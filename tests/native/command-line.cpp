// SplitCommandLine must agree with CommandLineToArgvW on every argument shape
// the host sees, so replacing it changes nothing but the SHELL32 load.
#include <windows.h>
#include <shellapi.h>

#include "../../src/WinBlaze.UI/src/CommandLine.h"

#include <iostream>

int wmain()
{
    const wchar_t* cases[] = {
        L"WinBlaze.UI.exe",
        L"\"C:\\Program Files\\WinBlaze\\WinBlaze.UI.exe\"",
        L"\"C:\\Program Files\\WinBlaze\\WinBlaze.UI.exe\" C:\\",
        L"WinBlaze.UI.exe \"C:\\scan exports\\full drive.csv\"",
        L"WinBlaze.UI.exe --treemap-png \"C:\\out\\map.png\"",
        L"WinBlaze.UI.exe \"C:\\trailing\\\\\"",
        L"WinBlaze.UI.exe \"C:\\\\\"",
        L"WinBlaze.UI.exe a\\\\\\\"b c\\\\d \"e\"\"f\" \"\"",
        L"WinBlaze.UI.exe   \t spaced   \"tab\targ\"  ",
        L"WinBlaze.UI.exe \u65e5\u672c\\\u30c6\u30b9\u30c8.csv",
        L"WinBlaze.UI.exe \"unterminated quote arg",
        L"WinBlaze.UI.exe a\"b\"c d\"\"e",
        L"x \"e\"\"f\"",
        L"x \"a\"\"\"\"b\"",
        L"x \"a\"\"\"b c\"",
    };
    int failures = 0;
    for (auto line : cases) {
        int count = 0;
        LPWSTR* expected = CommandLineToArgvW(line, &count);
        const auto actual = WinBlaze::UI::SplitCommandLine(line);
        bool same = expected && static_cast<int>(actual.size()) == count;
        for (int index = 0; same && index < count; ++index) {
            same = actual[static_cast<size_t>(index)] == expected[index];
        }
        if (!same) {
            ++failures;
            std::wcerr << L"FAIL: " << line << L"\n";
        }
        LocalFree(expected);
    }
    if (failures) return 1;
    std::wcout << L"PASS: command line splitting matches CommandLineToArgvW\n";
    return 0;
}
