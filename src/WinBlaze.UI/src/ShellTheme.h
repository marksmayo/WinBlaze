#pragma once

#include "pch.h"

namespace winrt::WinBlaze::UI::implementation
{
    struct ShellTheme
    {
        // Modern Dark Theme with Purple/Blue Gradient Accents
        
        // Lighter backgrounds for better text readability
        winrt::Windows::UI::Color app_background{ 0xFF, 0x1E, 0x1E, 0x2E };     // Lighter dark background
        winrt::Windows::UI::Color card_background{ 0xFF, 0x2D, 0x2D, 0x30 };    // Lighter card background
        winrt::Windows::UI::Color card_border{ 0x60, 0xFF, 0xFF, 0xFF };        // More visible border
        
        // High contrast text colors
        winrt::Windows::UI::Color text_primary{ 0xFF, 0xFF, 0xFF, 0xFF };       // Pure white
        winrt::Windows::UI::Color text_on_accent{ 0xFF, 0xFF, 0xFF, 0xFF };     // White on colored backgrounds
        
        // Readable subtle colors
        winrt::Windows::UI::Color subtle_background{ 0xFF, 0x3C, 0x3C, 0x3C };  // Lighter subtle background
        winrt::Windows::UI::Color subtle_border{ 0x80, 0xFF, 0xFF, 0xFF };      // More visible subtle border
        
        // Status Colors - Modern vibrant palette
        winrt::Windows::UI::Color error_background{ 0x33, 0xFF, 0x3B, 0x3B };   // Red with transparency
        winrt::Windows::UI::Color error_border{ 0xFF, 0xFF, 0x3B, 0x3B };       // Bright red
        
        // Interactive Elements - Better contrast
        winrt::Windows::UI::Color chip_background{ 0xFF, 0x4A, 0x4A, 0x4A };    // Lighter background
        winrt::Windows::UI::Color chip_active_background{ 0xFF, 0x6B, 0x5F, 0xFF }; // Electric purple
        winrt::Windows::UI::Color chip_inactive_background{ 0xFF, 0x3A, 0x3A, 0x3A }; // Lighter inactive background
        winrt::Windows::UI::Color chip_active_border{ 0xFF, 0x8B, 0x7F, 0xFF };  // Lighter purple border
        
        // Progress Colors - Better visibility
        winrt::Windows::UI::Color progress_track{ 0xFF, 0x4A, 0x4A, 0x4A };     // Lighter track
        winrt::Windows::UI::Color progress_fill{ 0xFF, 0x6B, 0x5F, 0xFF };      // Purple fill
        
        // File Type Colors - Vibrant modern palette
        winrt::Windows::UI::Color volume_accent{ 0xFF, 0x6B, 0x5F, 0xFF };      // Electric purple
        winrt::Windows::UI::Color folder_accent{ 0xFF, 0x4B, 0x7F, 0xFF };      // Bright blue
        winrt::Windows::UI::Color file_accent{ 0xFF, 0x00, 0xD6, 0x8F };        // Vibrant green
        winrt::Windows::UI::Color archive_accent{ 0xFF, 0xFF, 0xB8, 0x00 };     // Golden yellow
        
        // Modern Spacing and Radii
        double card_radius{ 12.0 };         // More rounded for modern feel
        double compact_card_radius{ 8.0 };  // Smaller cards still rounded
        double panel_radius{ 12.0 };        // Consistent with cards
        double chip_radius{ 8.0 };          // Rounded but not pill-shaped
        double progress_radius{ 4.0 };      // Subtle rounding
        double card_padding{ 20.0 };        // More generous padding
        double card_title_size{ 20.0 };     // Slightly larger for hierarchy
    };

    inline ShellTheme const& ActiveShellTheme()
    {
        static const ShellTheme theme{};
        return theme;
    }
}
