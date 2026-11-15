#pragma once
// Splits a Windows command line with the rules CommandLineToArgvW applies,
// without loading SHELL32 (the host delay-loads it, and the window parses its
// arguments before the first frame).
#include <string>
#include <string_view>
#include <vector>

namespace WinBlaze::UI
{
    inline std::vector<std::wstring> SplitCommandLine(std::wstring_view line)
    {
        std::vector<std::wstring> arguments;
        size_t at = 0;
        // The program name: quotes delimit it and backslashes are literal.
        std::wstring program;
        if (at < line.size() && line[at] == L'"') {
            ++at;
            while (at < line.size() && line[at] != L'"') program.push_back(line[at++]);
            if (at < line.size()) ++at;
        } else {
            while (at < line.size() && line[at] != L' ' && line[at] != L'\t') program.push_back(line[at++]);
        }
        arguments.push_back(std::move(program));
        for (;;) {
            while (at < line.size() && (line[at] == L' ' || line[at] == L'\t')) ++at;
            if (at >= line.size()) break;
            std::wstring argument;
            bool quoted = false;
            while (at < line.size()) {
                const wchar_t ch = line[at];
                if (!quoted && (ch == L' ' || ch == L'\t')) break;
                if (ch == L'\\') {
                    size_t slashes = 0;
                    while (at < line.size() && line[at] == L'\\') { ++slashes; ++at; }
                    if (at < line.size() && line[at] == L'"') {
                        argument.append(slashes / 2, L'\\');
                        if (slashes % 2) { argument.push_back(L'"'); ++at; }
                    } else {
                        argument.append(slashes, L'\\');
                    }
                    continue;
                }
                if (ch == L'"') {
                    ++at;
                    // Inside quotes a doubled quote is one literal quote and
                    // ends the quoted run (CommandLineToArgvW's rule).
                    if (quoted && at < line.size() && line[at] == L'"') {
                        argument.push_back(L'"');
                        ++at;
                        quoted = false;
                    } else {
                        quoted = !quoted;
                    }
                    continue;
                }
                argument.push_back(ch);
                ++at;
            }
            arguments.push_back(std::move(argument));
        }
        return arguments;
    }
}
