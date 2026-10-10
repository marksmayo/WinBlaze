#pragma once
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cwctype>
#include <limits>
#include <locale>
#include <optional>
#include <sstream>
#include <string>
namespace WinBlaze::UI {
    inline std::optional<uint64_t> ParseByteSize(std::wstring text) {
        text.erase(std::remove(text.begin(), text.end(), L','), text.end());
        std::transform(text.begin(), text.end(), text.begin(),
            [](wchar_t value) { return static_cast<wchar_t>(std::towlower(value)); });
        std::wistringstream input(text);
        input.imbue(std::locale::classic());
        long double value{};
        if (!(input >> value) || !std::isfinite(value) || value < 0) { return {}; }
        std::wstring unit;
        input >> unit;
        if (!input.eof()) {
            input >> std::ws;
            if (!input.eof()) { return {}; }
        }
        long double scale = 1;
        if (unit == L"t" || unit == L"tb" || unit == L"tib") { scale = 1099511627776.0L; }
        else if (unit == L"g" || unit == L"gb" || unit == L"gib") { scale = 1073741824.0L; }
        else if (unit == L"m" || unit == L"mb" || unit == L"mib") { scale = 1048576.0L; }
        else if (unit == L"k" || unit == L"kb" || unit == L"kib") { scale = 1024.0L; }
        else if (!unit.empty() && unit != L"b" && unit != L"byte" && unit != L"bytes") { return {}; }
        const auto bytes = value * scale;
        // MSVC long double is double: 2^64 itself cannot be converted safely.
        if (!std::isfinite(bytes) || bytes >= 18446744073709551616.0L) { return {}; }
        return static_cast<uint64_t>(bytes);
    }
}
