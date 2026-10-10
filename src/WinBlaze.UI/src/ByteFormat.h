#pragma once

#include <cstdint>
#include <charconv>
#include <stdexcept>
#include <string>
#include <string_view>

namespace WinBlaze::UI
{
    inline std::wstring FormatByteSize(uint64_t size)
    {
        constexpr double KB = 1024.0;
        constexpr double MB = KB * 1024.0;
        constexpr double GB = MB * 1024.0;
        const double value = static_cast<double>(size);
        double amount = value;
        std::wstring_view suffix = L" B";
        int precision = 0;
        if (value >= GB) {
            amount = value / GB;
            suffix = L" GB";
            precision = 1;
        } else if (value >= MB) {
            amount = value / MB;
            suffix = L" MB";
            precision = 1;
        } else if (value >= KB) {
            amount = value / KB;
            suffix = L" KB";
            precision = 1;
        }
        // The UI uses classic decimal formatting. Keep the original double
        // conversion and precision, including rounding near unit boundaries.
        char digits[64];
        const auto result = std::to_chars(digits, digits + sizeof(digits), amount,
            std::chars_format::fixed, precision);
        if (result.ec != std::errc{}) throw std::length_error("Byte label exceeds buffer");
        const auto count = static_cast<size_t>(result.ptr - digits);
        std::wstring text(count + suffix.size(), L'\0');
        for (size_t index = 0; index < count; ++index) text[index] = digits[index];
        for (size_t index = 0; index < suffix.size(); ++index) text[count + index] = suffix[index];
        return text;
    }
}
