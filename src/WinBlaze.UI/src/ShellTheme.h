#pragma once

#include "pch.h"

namespace winrt::WinBlaze::UI::implementation
{
    enum class ThemeVariant
    {
        GitHubDark,
        GitHubLight,
        HighContrast,
        WinBlazePurple
    };

    struct ShellTheme
    {
        // WinBlaze Design System - Configurable Theme
        ThemeVariant variant{ ThemeVariant::GitHubDark };
        
        // Theme colors (will be set based on variant)
        winrt::Windows::UI::Color app_background{ 0xFF, 0x13, 0x13, 0x13 };     // #131313 - Carbon bg
        winrt::Windows::UI::Color card_background{ 0xFF, 0x20, 0x20, 0x20 };    // #202020 - Surface container bg
        winrt::Windows::UI::Color card_border{ 0xFF, 0x2A, 0x2A, 0x2A };        // #2A2A2A - Acrylic/outline border
        
        // Stitch-style text colors
        winrt::Windows::UI::Color text_primary{ 0xFF, 0xE5, 0xE2, 0xE1 };       // #E5E2E1 - On-surface light text
        winrt::Windows::UI::Color text_secondary{ 0xFF, 0xC0, 0xC7, 0xD4 };     // #C0C7D4 - On-surface-variant text
        winrt::Windows::UI::Color text_on_accent{ 0xFF, 0xFF, 0xFF, 0xFF };     // White on accent
        
        // Stitch secondary colors
        winrt::Windows::UI::Color subtle_background{ 0xFF, 0x25, 0x25, 0x25 };  // #252525 - Subtle container
        winrt::Windows::UI::Color subtle_border{ 0xFF, 0x2A, 0x2A, 0x2A };      // #2A2A2A - Outline/acrylic border
        
        // Status Colors
        winrt::Windows::UI::Color error_background{ 0x33, 0xFF, 0xB4, 0xAB };   // Error background (Stitch error peach)
        winrt::Windows::UI::Color error_border{ 0xFF, 0xFF, 0xB4, 0xAB };       // Error border
        
        // Interactive Elements - Electric Blue Theme
        winrt::Windows::UI::Color chip_background{ 0xFF, 0x25, 0x25, 0x25 };    // Button bg
        winrt::Windows::UI::Color chip_active_background{ 0xFF, 0x00, 0x78, 0xD4 }; // #0078D4 - Electric Blue
        winrt::Windows::UI::Color chip_inactive_background{ 0xFF, 0x25, 0x25, 0x25 }; // Inactive bg
        winrt::Windows::UI::Color chip_active_border{ 0xFF, 0x00, 0x78, 0xD4 };  // Active border
        
        // Progress Colors - Electric Blue Theme
        winrt::Windows::UI::Color progress_track{ 0xFF, 0x25, 0x25, 0x25 };     // Track bg
        winrt::Windows::UI::Color progress_fill{ 0xFF, 0x00, 0x78, 0xD4 };      // #0078D4 - Electric Blue
        
        // File Type Colors - Stitch Palette
        winrt::Windows::UI::Color volume_accent{ 0xFF, 0x00, 0x78, 0xD4 };      // #0078D4 - Electric Blue for volumes
        winrt::Windows::UI::Color folder_accent{ 0xFF, 0x4B, 0xD9, 0xE5 };      // #4BD9E5 - Cyan for folders
        winrt::Windows::UI::Color file_accent{ 0xFF, 0xA3, 0xC9, 0xFF };        // #A3C9FF - Light blue for files
        winrt::Windows::UI::Color archive_accent{ 0xFF, 0xFF, 0xB4, 0xAB };     // #FFB4AB - Peach for archives
        
        // Design System Spacing and Radii - 8px corner radius
        double card_radius{ 8.0 };          // 8px as per design system
        double compact_card_radius{ 6.0 };  // Slightly smaller for compact cards
        double panel_radius{ 8.0 };         // Consistent 8px radius
        double chip_radius{ 8.0 };          // 8px for chips
        double progress_radius{ 4.0 };      // Subtle rounding for progress
        double card_padding{ 16.0 };        // Clean, generous padding
        double card_title_size{ 18.0 };     // Clear hierarchy
    };

    inline ShellTheme CreateTheme(ThemeVariant variant)
    {
        ShellTheme theme{};
        theme.variant = variant;
        
        switch (variant)
        {
        case ThemeVariant::GitHubDark:
            // Default Stitch/Mica Dark theme
            theme.app_background = { 0xFF, 0x13, 0x13, 0x13 };     // #131313 - Carbon bg
            theme.card_background = { 0xFF, 0x20, 0x20, 0x20 };    // #202020 - Surface container bg
            theme.card_border = { 0xFF, 0x2A, 0x2A, 0x2A };        // #2A2A2A - Acrylic border
            theme.text_primary = { 0xFF, 0xE5, 0xE2, 0xE1 };       // #E5E2E1 - On-surface
            theme.text_secondary = { 0xFF, 0xC0, 0xC7, 0xD4 };     // #C0C7D4 - On-surface-variant
            theme.subtle_background = { 0xFF, 0x25, 0x25, 0x25 };  // #252525 - Subtle bg
            theme.chip_background = { 0xFF, 0x25, 0x25, 0x25 };    // Chip bg
            theme.chip_active_background = { 0xFF, 0x00, 0x78, 0xD4 }; // #0078D4 - Electric Blue
            break;
            
        case ThemeVariant::GitHubLight:
            theme.app_background = { 0xFF, 0xFF, 0xFF, 0xFF };     // White
            theme.card_background = { 0xFF, 0xF6, 0xF8, 0xFA };    // #F6F8FA
            theme.card_border = { 0xFF, 0xD0, 0xD7, 0xDE };        // #D0D7DE
            theme.text_primary = { 0xFF, 0x24, 0x29, 0x2F };       // #24292F
            theme.text_secondary = { 0xFF, 0x57, 0x60, 0x6A };     // #57606A
            theme.subtle_background = { 0xFF, 0xF6, 0xF8, 0xFA };  // #F6F8FA
            theme.chip_background = { 0xFF, 0xF6, 0xF8, 0xFA };    // #F6F8FA
            theme.chip_active_background = { 0xFF, 0x1F, 0x6F, 0xEB }; // #1F6FEB
            break;
            
        case ThemeVariant::HighContrast:
            theme.app_background = { 0xFF, 0x00, 0x00, 0x00 };     // Black
            theme.card_background = { 0xFF, 0x1A, 0x1A, 0x1A };    // Dark gray
            theme.card_border = { 0xFF, 0xFF, 0xFF, 0xFF };        // White
            theme.text_primary = { 0xFF, 0xFF, 0xFF, 0xFF };       // White
            theme.text_secondary = { 0xFF, 0xFF, 0xFF, 0x00 };     // Yellow
            theme.subtle_background = { 0xFF, 0x33, 0x33, 0x33 };  // Medium gray
            theme.chip_background = { 0xFF, 0x33, 0x33, 0x33 };    // Medium gray
            theme.chip_active_background = { 0xFF, 0x00, 0xFF, 0xFF }; // Cyan
            break;
            
        case ThemeVariant::WinBlazePurple:
            theme.app_background = { 0xFF, 0x1A, 0x0B, 0x2E };     // Deep purple
            theme.card_background = { 0xFF, 0x2D, 0x1B, 0x45 };    // Purple card
            theme.card_border = { 0xFF, 0x4C, 0x2F, 0x6E };        // Purple border
            theme.text_primary = { 0xFF, 0xE9, 0xE4, 0xF0 };       // Light purple
            theme.text_secondary = { 0xFF, 0xB3, 0x9D, 0xDB };     // Medium purple
            theme.subtle_background = { 0xFF, 0x3A, 0x2A, 0x56 };  // Subtle purple
            theme.chip_background = { 0xFF, 0x3A, 0x2A, 0x56 };    // Subtle purple
            theme.chip_active_background = { 0xFF, 0x8B, 0x5C, 0xF6 }; // Bright purple
            break;
        }
        
        return theme;
    }

    inline ShellTheme const& ActiveShellTheme()
    {
        static const ShellTheme theme = CreateTheme(ThemeVariant::GitHubDark);
        return theme;
    }

    inline void SetActiveTheme(ThemeVariant variant)
    {
        // Note: For now, this is simplified and would require UI refresh
        // In a real implementation, you'd store this and refresh the UI
    }

    inline ThemeVariant GetActiveThemeVariant()
    {
        return ThemeVariant::GitHubDark;
    }

    // Fixed categorical palette for the per-extension breakdown table.
    // Chosen for perceptual distinctness against the dark card background
    // rather than semantic meaning (unlike volume/folder/file accents).
    inline winrt::Windows::UI::Color ExtensionSwatchColor(std::wstring_view extension)
    {
        static constexpr winrt::Windows::UI::Color kPalette[] = {
            { 0xFF, 0x00, 0x78, 0xD4 }, // electric blue
            { 0xFF, 0x4B, 0xD9, 0xE5 }, // cyan
            { 0xFF, 0xFF, 0xB4, 0xAB }, // peach
            { 0xFF, 0x8B, 0xC3, 0x4A }, // lime green
            { 0xFF, 0xFF, 0xC1, 0x07 }, // amber
            { 0xFF, 0xE0, 0x5D, 0xC4 }, // magenta
            { 0xFF, 0xFF, 0x8A, 0x3D }, // orange
            { 0xFF, 0x9C, 0x7B, 0xF6 }, // violet
            { 0xFF, 0x2E, 0xC7, 0x9B }, // teal
            { 0xFF, 0xF0, 0x5C, 0x5C }, // coral red
            { 0xFF, 0xA3, 0xC9, 0xFF }, // light blue
            { 0xFF, 0xD4, 0xE1, 0x57 }, // yellow-green
        };
        constexpr size_t paletteSize = sizeof(kPalette) / sizeof(kPalette[0]);

        size_t hash = 146527;
        for (wchar_t ch : extension) {
            hash = hash * 31 + static_cast<size_t>(ch);
        }
        return kPalette[hash % paletteSize];
    }
}
