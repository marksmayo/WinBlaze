#pragma once
#include <windows.h>
#include <shellapi.h>
#include <string>
#include <filesystem>
#include <vector>
#include <algorithm>

namespace WinBlaze::Actions {
inline DWORD CheckFile(std::wstring const& path) {
    if(path.empty()||path.find(L'\0')!=std::wstring::npos||path.find_first_of(L"*?")!=std::wstring::npos)return ERROR_INVALID_NAME;
    auto attributes=GetFileAttributesW(path.c_str());
    if(attributes==INVALID_FILE_ATTRIBUTES)return GetLastError();
    return attributes&(FILE_ATTRIBUTE_DIRECTORY|FILE_ATTRIBUTE_REPARSE_POINT)?ERROR_ACCESS_DENIED:0;
}
inline DWORD Rename(std::wstring const& input, std::wstring const& name) {
    if (name.empty() || name == L"." || name == L".." ||
        name.find(L'\0') != std::wstring::npos ||
        name.find_first_of(L"\\/:*?\"<>|") != std::wstring::npos ||
        name.back() == L'.' || name.back() == L' ') return ERROR_INVALID_NAME;
    if (input.empty() || input.find(L'\0') != std::wstring::npos) return ERROR_INVALID_NAME;
    auto attributes = GetFileAttributesW(input.c_str());
    if (attributes == INVALID_FILE_ATTRIBUTES) return GetLastError();
    if (attributes & FILE_ATTRIBUTE_REPARSE_POINT) return ERROR_ACCESS_DENIED;
    auto source = std::filesystem::absolute(std::filesystem::path(input)).lexically_normal();
    if (source == source.root_path()) return ERROR_ACCESS_DENIED;
    auto destination = source.parent_path() / name;
    if (MoveFileExW(source.c_str(), destination.c_str(), 0)) return 0;
    return GetLastError();
}
inline DWORD Move(std::wstring const& source,std::wstring const& destination) {
    if(auto error=CheckFile(source))return error;
    if(destination.empty()||destination.find(L'\0')!=std::wstring::npos)return ERROR_INVALID_NAME;
    if(MoveFileExW(source.c_str(),destination.c_str(),MOVEFILE_COPY_ALLOWED))return 0;
    return GetLastError();
}
inline DWORD Recycle(std::wstring const& input) {
    if(auto error=CheckFile(input))return error;
    wchar_t buffer[32768]{};
    auto length=GetFullPathNameW(input.c_str(),32768,buffer,nullptr);
    if(!length||length>=32768)return ERROR_INVALID_NAME;
    std::wstring source(buffer,length);source.push_back(L'\0');
    SHFILEOPSTRUCTW operation{};operation.wFunc=FO_DELETE;operation.pFrom=source.c_str();
    // Windows must still warn if recycling is impossible; no silent permanent
    // fallback, connected-folder deletion or recursive directory operations.
    operation.fFlags=FOF_ALLOWUNDO|FOF_NOCONFIRMATION|FOF_WANTNUKEWARNING|FOF_NO_CONNECTED_ELEMENTS|FOF_NORECURSION;
    auto result=SHFileOperationW(&operation);
    return operation.fAnyOperationsAborted?ERROR_CANCELLED:static_cast<DWORD>(result);
}
// Validate every source before starting a batch. Roots, reparse points and
// parent/child or duplicate selections are rejected.
inline DWORD ValidateItems(std::vector<std::wstring> const& inputs,
    std::vector<std::filesystem::path>& paths) {
    if (inputs.empty()) return ERROR_INVALID_PARAMETER;
    try {
        for (auto const& input : inputs) {
            if (input.empty() || input.find(L'\0') != std::wstring::npos ||
                input.find_first_of(L"*?") != std::wstring::npos) return ERROR_INVALID_NAME;
            auto path = std::filesystem::absolute(input).lexically_normal();
            while (path.has_relative_path() && path.filename().empty()) path = path.parent_path();
            if (path == path.root_path()) return ERROR_ACCESS_DENIED;
            auto attributes = GetFileAttributesW(path.c_str());
            if (attributes == INVALID_FILE_ATTRIBUTES) return GetLastError();
            if (attributes & FILE_ATTRIBUTE_REPARSE_POINT) return ERROR_ACCESS_DENIED;
            auto lowered = path.wstring();
            std::transform(lowered.begin(), lowered.end(), lowered.begin(), towlower);
            for (auto const& prior : paths) {
                auto other = prior.wstring();
                std::transform(other.begin(), other.end(), other.begin(), towlower);
                if (other == lowered || other.rfind(lowered + L"\\", 0) == 0 ||
                    lowered.rfind(other + L"\\", 0) == 0) return ERROR_INVALID_PARAMETER;
            }
            paths.push_back(std::move(path));
        }
    } catch (...) { return ERROR_INVALID_NAME; }
    return 0;
}
inline DWORD MoveItems(std::vector<std::wstring> const& inputs, std::wstring const& destination) {
    std::vector<std::filesystem::path> paths;
    if (auto error = ValidateItems(inputs, paths)) return error;
    auto attributes = GetFileAttributesW(destination.c_str());
    if (attributes == INVALID_FILE_ATTRIBUTES) return GetLastError();
    if (!(attributes & FILE_ATTRIBUTE_DIRECTORY) || attributes & FILE_ATTRIBUTE_REPARSE_POINT)
        return ERROR_ACCESS_DENIED;
    std::vector<std::filesystem::path> targets;
    try {
        auto folder = std::filesystem::absolute(destination).lexically_normal();
        for (auto const& path : paths) {
            auto target = folder / path.filename();
            if (GetFileAttributesW(target.c_str()) != INVALID_FILE_ATTRIBUTES) return ERROR_ALREADY_EXISTS;
            if (std::find(targets.begin(), targets.end(), target) != targets.end()) return ERROR_ALREADY_EXISTS;
            targets.push_back(std::move(target));
        }
        for (size_t index = 0; index < paths.size(); ++index) {
            // No overwrite. Cross-volume directory moves fail safely.
            if (!MoveFileExW(paths[index].c_str(), targets[index].c_str(), MOVEFILE_COPY_ALLOWED))
                return GetLastError();
        }
    } catch (...) { return ERROR_INVALID_NAME; }
    return 0;
}
inline DWORD RecycleItems(std::vector<std::wstring> const& inputs) {
    std::vector<std::filesystem::path> paths;
    if (auto error = ValidateItems(inputs, paths)) return error;
    std::wstring sources;
    for (auto const& path : paths) { sources += path.wstring(); sources.push_back(L'\0'); }
    sources.push_back(L'\0');
    SHFILEOPSTRUCTW operation{};
    operation.wFunc = FO_DELETE;
    operation.pFrom = sources.data();
    // Request the Recycle Bin and retain Windows' permanent-delete warning.
    operation.fFlags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING |
        FOF_NO_CONNECTED_ELEMENTS;
    auto result = SHFileOperationW(&operation);
    return operation.fAnyOperationsAborted ? ERROR_CANCELLED : static_cast<DWORD>(result);
}

}
