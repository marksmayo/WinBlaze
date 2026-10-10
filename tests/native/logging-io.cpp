#include "../../src/WinBlaze.UI/src/StartupTrace.h"
#include <fstream>
#include <iostream>
#include <iterator>

int wmain(int argc, wchar_t** argv)
{
    if (argc != 2) { return 2; }
    const std::wstring path = argv[1];
    DeleteFileW(path.c_str());
    IO_COUNTERS before{}, after{};
    if (!GetProcessIoCounters(GetCurrentProcess(), &before)) { return 3; }
    const std::string line = "trace UTF-8 \xC3\xA4";
    for (int i = 0; i < 10; ++i) {
        WinBlazeWriteUtf8Line(path, line);
    }
    if (!GetProcessIoCounters(GetCurrentProcess(), &after)) { return 4; }
    const auto writes = after.WriteOperationCount - before.WriteOperationCount;
    std::ifstream input(path, std::ios::binary);
    const std::string actual((std::istreambuf_iterator<char>(input)), {});
    std::string expected;
    for (int i = 0; i < 10; ++i) { expected += line + "\r\n"; }
    if (actual != expected) { std::cerr << "Log content mismatch\n"; return 5; }
    if (writes > 10) {
        std::cerr << "Expected <=10 log writes; measured " << writes << "\n";
        return 1;
    }
    input.close();
    DeleteFileW(path.c_str());
    const std::string longLine(1200, 'x');
    WinBlazeWriteUtf8Line(path, longLine);
    WinBlazeWriteUtf8Line(path, "");
    std::ifstream longInput(path, std::ios::binary);
    const std::string longActual((std::istreambuf_iterator<char>(longInput)), {});
    if (longActual != longLine + "\r\n\r\n") { return 6; }
    std::cout << "PASS: UTF-8/CRLF, long/empty lines, " << writes << " short-line write operations\n";
    return 0;
}
