#pragma once

#include "pch.h"

namespace winrt::WinBlaze::UI::implementation
{
    struct ShellTheme
    {
        winrt::Windows::UI::Color app_background{ winrt::Windows::UI::Colors::WhiteSmoke() };
        winrt::Windows::UI::Color card_background{ winrt::Windows::UI::Colors::White() };
        winrt::Windows::UI::Color card_border{ winrt::Windows::UI::Colors::SlateGray() };
        winrt::Windows::UI::Color text_primary{ winrt::Windows::UI::Colors::Black() };
        winrt::Windows::UI::Color text_on_accent{ winrt::Windows::UI::Colors::White() };
        winrt::Windows::UI::Color subtle_background{ winrt::Windows::UI::Colors::WhiteSmoke() };
        winrt::Windows::UI::Color subtle_border{ winrt::Windows::UI::Colors::LightSteelBlue() };
        winrt::Windows::UI::Color error_background{ winrt::Windows::UI::Colors::MistyRose() };
        winrt::Windows::UI::Color error_border{ winrt::Windows::UI::Colors::IndianRed() };
        winrt::Windows::UI::Color chip_background{ winrt::Windows::UI::Colors::LightSteelBlue() };
        winrt::Windows::UI::Color chip_active_background{ winrt::Windows::UI::Colors::LightSkyBlue() };
        winrt::Windows::UI::Color chip_inactive_background{ winrt::Windows::UI::Colors::Gainsboro() };
        winrt::Windows::UI::Color chip_active_border{ winrt::Windows::UI::Colors::SteelBlue() };
        winrt::Windows::UI::Color progress_track{ winrt::Windows::UI::Colors::Gainsboro() };
        winrt::Windows::UI::Color progress_fill{ winrt::Windows::UI::Colors::RoyalBlue() };
        winrt::Windows::UI::Color volume_accent{ winrt::Windows::UI::Colors::CornflowerBlue() };
        winrt::Windows::UI::Color folder_accent{ winrt::Windows::UI::Colors::SeaGreen() };
        winrt::Windows::UI::Color file_accent{ winrt::Windows::UI::Colors::MediumPurple() };
        winrt::Windows::UI::Color archive_accent{ winrt::Windows::UI::Colors::Chocolate() };
        double card_radius{ 8.0 };
        double compact_card_radius{ 8.0 };
        double panel_radius{ 8.0 };
        double chip_radius{ 999.0 };
        double progress_radius{ 4.0 };
        double card_padding{ 16.0 };
        double card_title_size{ 18.0 };
    };

    inline ShellTheme const& ActiveShellTheme()
    {
        static const ShellTheme theme{};
        return theme;
    }
}
