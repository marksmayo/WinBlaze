#include "../../src/WinBlaze.UI/src/StartupTrace.h"
#include <iostream>
#include <fstream>
#include <iterator>
int wmain(int argc, wchar_t** argv)
{
    if (argc == 3 && std::wstring_view(argv[1]) == L"timestamps") {
        SetEnvironmentVariableW(L"TEMP", argv[2]);
        SetEnvironmentVariableW(L"TMP", argv[2]);
        SetEnvironmentVariableW(L"WINBLAZE_STARTUP_TRACE", L"1");
        LARGE_INTEGER before{}, after{}, frequency{};
        QueryPerformanceFrequency(&frequency);
        QueryPerformanceCounter(&before);
        TraceStartup(L"Phase one");
        TraceStartup(L"Phase two");
        QueryPerformanceCounter(&after);
        const auto path = std::wstring(argv[2]) + L"\\WinBlaze-startup.log";
        std::ifstream input(path, std::ios::binary);
        std::string line;
        unsigned int count = 0;
        long long previous = before.QuadPart;
        while (std::getline(input, line)) {
            const auto pid = "\"pid\":" + std::to_string(GetCurrentProcessId());
            const auto hz = "\"qpc_frequency\":" + std::to_string(frequency.QuadPart);
            const auto position = line.find("\"qpc\":");
            if (line.find(pid) == std::string::npos || line.find(hz) == std::string::npos
                || position == std::string::npos) {
                std::cerr << "Startup phases lack process/counter identity\n"; return 8;
            }
            const auto counter = std::stoll(line.substr(position + 6));
            if (counter < previous || counter > after.QuadPart) return 9;
            if (line.find(count == 0 ? "Phase one" : "Phase two") == std::string::npos) return 10;
            previous = counter;
            ++count;
        }
        if (count != 2) return 11;
        std::cout << "PASS: startup phases carry ordered counters and process identity\n";
        return 0;
    }
    if (argc == 3 && std::wstring_view(argv[1]) == L"failure") {
        SetEnvironmentVariableW(L"LOCALAPPDATA", argv[2]);
        SetEnvironmentVariableW(L"WINBLAZE_STARTUP_TRACE", nullptr);
        ReportFailure(L"startup regression", L"failure remains recorded");
        std::ifstream input(WinBlazeFailureLogPath(), std::ios::binary);
        std::string content((std::istreambuf_iterator<char>(input)), {});
        if (content.find("failure remains recorded") == std::string::npos) return 7;
        std::cout << "PASS: failure logged with routine tracing disabled\n"; return 0;
    }
    if (argc != 2) return 2;
    const bool enabled = std::wstring_view(argv[1]) == L"enabled";
    SetEnvironmentVariableW(L"WINBLAZE_STARTUP_TRACE", enabled ? L"1" : nullptr);
    IO_COUNTERS before{}, after{};
    GetProcessIoCounters(GetCurrentProcess(), &before);
    for (int i = 0; i < 50; ++i) TraceStartup(L"Startup trace regression");
    GetProcessIoCounters(GetCurrentProcess(), &after);
    const auto writes = after.WriteOperationCount - before.WriteOperationCount;
    if ((!enabled && writes != 0) || (enabled && writes != 50)) {
        std::cerr << "Unexpected trace writes: " << writes << "\n"; return 1;
    }
    std::cout << "PASS: routine trace writes=" << writes << "\n";
}
