#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <string_view>

inline constexpr uint64_t WinBlazeMaxStartupLogBytes = 1ULL * 1024ULL * 1024ULL;
inline constexpr uint64_t WinBlazeMaxFailureLogBytes = 2ULL * 1024ULL * 1024ULL;

inline std::string WinBlazeWideToUtf8(std::wstring_view text)
{
    std::string utf8;
    utf8.resize(WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), nullptr, 0, nullptr, nullptr));
    if (!utf8.empty()) {
        WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), utf8.data(), static_cast<int>(utf8.size()), nullptr, nullptr);
    }
    return utf8;
}

inline std::string WinBlazeJsonEscape(std::wstring_view text)
{
    std::string escaped;
    for (char ch : WinBlazeWideToUtf8(text)) {
        switch (ch) {
        case '\\':
            escaped += "\\\\";
            break;
        case '"':
            escaped += "\\\"";
            break;
        case '\n':
            escaped += "\\n";
            break;
        case '\r':
            escaped += "\\r";
            break;
        case '\t':
            escaped += "\\t";
            break;
        default:
            escaped += ch;
            break;
        }
    }
    return escaped;
}

inline uint64_t WinBlazeUnixTimeMs()
{
    FILETIME file_time{};
    GetSystemTimeAsFileTime(&file_time);
    ULARGE_INTEGER value{};
    value.LowPart = file_time.dwLowDateTime;
    value.HighPart = file_time.dwHighDateTime;
    constexpr uint64_t windows_to_unix_100ns = 116444736000000000ULL;
    return (value.QuadPart - windows_to_unix_100ns) / 10000ULL;
}

inline void WinBlazeWriteUtf8Line(std::wstring const& path, std::string_view line)
{
    WIN32_FILE_ATTRIBUTE_DATA attributes{};
    if (GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &attributes)) {
        ULARGE_INTEGER size{};
        size.LowPart = attributes.nFileSizeLow;
        size.HighPart = attributes.nFileSizeHigh;
        const bool is_failure_log = path.size() >= 14 &&
            path.compare(path.size() - 14, 14, L"failures.jsonl") == 0;
        const uint64_t max_bytes = is_failure_log
            ? WinBlazeMaxFailureLogBytes
            : WinBlazeMaxStartupLogBytes;
        if (size.QuadPart >= max_bytes) {
            std::wstring rotated = path + L".1";
            DeleteFileW(rotated.c_str());
            MoveFileExW(path.c_str(), rotated.c_str(), MOVEFILE_REPLACE_EXISTING);
        }
    }

    HANDLE file = CreateFileW(
        path.c_str(),
        FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        nullptr,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);

    if (file == INVALID_HANDLE_VALUE) {
        return;
    }

    const char newline[] = "\r\n";
    DWORD written = 0;
    WriteFile(file, line.data(), static_cast<DWORD>(line.size()), &written, nullptr);
    WriteFile(file, newline, sizeof(newline) - 1, &written, nullptr);

    CloseHandle(file);
}

inline void TraceStartup(std::wstring_view message)
{
    WCHAR temp_path[MAX_PATH]{};
    const DWORD temp_length = GetTempPathW(MAX_PATH, temp_path);
    if (temp_length == 0 || temp_length >= MAX_PATH) {
        return;
    }

    std::wstring log_path(temp_path, temp_path + temp_length);
    log_path += L"WinBlaze-startup.log";
    WinBlazeWriteUtf8Line(log_path, WinBlazeWideToUtf8(message));
}

inline std::wstring WinBlazeFailureLogPath()
{
    WCHAR local_app_data[MAX_PATH]{};
    const DWORD length = GetEnvironmentVariableW(L"LOCALAPPDATA", local_app_data, MAX_PATH);
    std::wstring root = (length > 0 && length < MAX_PATH)
        ? std::wstring(local_app_data, local_app_data + length)
        : std::wstring{};
    if (root.empty()) {
        WCHAR temp_path[MAX_PATH]{};
        const DWORD temp_length = GetTempPathW(MAX_PATH, temp_path);
        if (temp_length == 0 || temp_length >= MAX_PATH) {
            return {};
        }
        root.assign(temp_path, temp_path + temp_length);
    }

    std::wstring app_dir = root + L"\\WinBlaze";
    std::wstring log_dir = app_dir + L"\\logs";
    CreateDirectoryW(app_dir.c_str(), nullptr);
    CreateDirectoryW(log_dir.c_str(), nullptr);
    return log_dir + L"\\failures.jsonl";
}

inline void ReportFailure(std::wstring_view stage, std::wstring_view message)
{
    const std::wstring path = WinBlazeFailureLogPath();
    if (path.empty()) {
        return;
    }

    std::string line = "{\"ts_ms\":" + std::to_string(WinBlazeUnixTimeMs()) +
        ",\"component\":\"ui\",\"stage\":\"" + WinBlazeJsonEscape(stage) +
        "\",\"message\":\"" + WinBlazeJsonEscape(message) + "\"}";
    WinBlazeWriteUtf8Line(path, line);
}

inline LONG WINAPI WinBlazeUnhandledExceptionFilter(EXCEPTION_POINTERS* exception_pointers)
{
    DWORD code = 0;
    if (exception_pointers && exception_pointers->ExceptionRecord) {
        code = exception_pointers->ExceptionRecord->ExceptionCode;
    }
    std::wstring message = L"Unhandled SEH exception code ";
    message += std::to_wstring(code);
    ReportFailure(L"unhandled_exception", message);
    return EXCEPTION_EXECUTE_HANDLER;
}
