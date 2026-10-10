# Startup CPU and working-set targets

50 concrete startup CPU/working-set targets: four imports and 46 shell color bindings, addressed through five shared production changes.

Import page-in opportunities first; direct bindings ordered by palette repetition, then shared card backgrounds/borders. Engineering priority order, not a measured per-target savings ranking.

WaitForInputIdle can precede shell construction. Brush changes may improve visible-ready CPU/memory without changing first idle; WinUI may independently load graphics DLLs. No working-set trimming or benchmark delays.

| Rank | Target | Source | Mechanism |
|---:|---|---|---|
| 1 | First-use VirtDisk.dll | src/WinBlaze.UI/WinBlaze.UI.vcxproj | first-use-imports |
| 2 | First-use d3d11.dll | src/WinBlaze.UI/WinBlaze.UI.vcxproj | first-use-imports |
| 3 | First-use d2d1.dll | src/WinBlaze.UI/WinBlaze.UI.vcxproj | first-use-imports |
| 4 | First-use DWrite.dll | src/WinBlaze.UI/WinBlaze.UI.vcxproj | first-use-imports |
| 5 | version_caption.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:913 | bounded-color-resources |
| 6 | item.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:933 | bounded-color-resources |
| 7 | item.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:966 | bounded-color-resources |
| 8 | m_progress_text.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:1174 | bounded-color-resources |
| 9 | m_selection_status_text.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:1196 | bounded-color-resources |
| 10 | m_root_validation_text.Foreground(make_brush(theme.text_secondary)); | src/WinBlaze.UI/src/MainWindow.cpp:1435 | bounded-color-resources |
| 11 | wordmark.Foreground(make_brush(theme.chip_active_background)); | src/WinBlaze.UI/src/MainWindow.cpp:907 | bounded-color-resources |
| 12 | item.Background(make_brush(theme.chip_active_background)); | src/WinBlaze.UI/src/MainWindow.cpp:928 | bounded-color-resources |
| 13 | item.Foreground(make_brush(theme.text_on_accent)); | src/WinBlaze.UI/src/MainWindow.cpp:929 | bounded-color-resources |
| 14 | item.Background(make_brush(Windows::UI::Colors::Transparent())); | src/WinBlaze.UI/src/MainWindow.cpp:932 | bounded-color-resources |
| 15 | item.Background(make_brush(Windows::UI::Colors::Transparent())); | src/WinBlaze.UI/src/MainWindow.cpp:965 | bounded-color-resources |
| 16 | button.Background(make_brush(Windows::UI::Colors::Transparent())); | src/WinBlaze.UI/src/MainWindow.cpp:1014 | bounded-color-resources |
| 17 | m_status_text.Foreground(make_brush(theme.text_primary)); | src/WinBlaze.UI/src/MainWindow.cpp:1167 | bounded-color-resources |
| 18 | m_start_scan_button.Background(make_brush(theme.chip_active_background)); | src/WinBlaze.UI/src/MainWindow.cpp:1369 | bounded-color-resources |
| 19 | m_start_scan_button.Foreground(make_brush(theme.text_on_accent)); | src/WinBlaze.UI/src/MainWindow.cpp:1370 | bounded-color-resources |
| 20 | title_text.Foreground(make_brush(theme.text_primary)); | src/WinBlaze.UI/src/MainWindow.cpp:1467 | bounded-color-resources |
| 21 | label.Foreground(make_brush(theme.text_primary)); | src/WinBlaze.UI/src/MainWindow.cpp:1822 | bounded-color-resources |
| 22 | label.Foreground(make_brush(theme.text_primary)); | src/WinBlaze.UI/src/MainWindow.cpp:1887 | bounded-color-resources |
| 23 | title_text.Foreground(make_brush(theme.text_on_accent)); | src/WinBlaze.UI/src/MainWindow.cpp:1951 | bounded-color-resources |
| 24 | subtitle_text.Foreground(make_brush(theme.text_on_accent)); | src/WinBlaze.UI/src/MainWindow.cpp:1956 | bounded-color-resources |
| 25 | m_treemap_zoom_overlay.Background(make_brush(Windows::UI::Colors::Transparent())); | src/WinBlaze.UI/src/MainWindow.cpp:2090 | bounded-color-resources |
| 26 | m_treemap_zoom_overlay.BorderBrush(make_brush(theme.chip_active_background)); | src/WinBlaze.UI/src/MainWindow.cpp:2091 | bounded-color-resources |
| 27 | sidebar_host.BorderBrush(make_brush(theme.card_border)); | src/WinBlaze.UI/src/MainWindow.cpp:884 | bounded-color-resources |
| 28 | menu_host.BorderBrush(make_brush(theme.card_border)); | src/WinBlaze.UI/src/MainWindow.cpp:998 | bounded-color-resources |
| 29 | footer_bar.BorderBrush(make_brush(theme.card_border)); | src/WinBlaze.UI/src/MainWindow.cpp:1153 | bounded-color-resources |
| 30 | root.Background(make_brush(theme.app_background)); | src/WinBlaze.UI/src/MainWindow.cpp:854 | bounded-color-resources |
| 31 | footer_bar.Background(make_brush(theme.app_background)); | src/WinBlaze.UI/src/MainWindow.cpp:1152 | bounded-color-resources |
| 32 | sidebar_host.Background(make_brush(theme.chip_background)); | src/WinBlaze.UI/src/MainWindow.cpp:883 | bounded-color-resources |
| 33 | m_sidebar_status_text.Foreground(make_brush(theme.folder_accent)); | src/WinBlaze.UI/src/MainWindow.cpp:980 | bounded-color-resources |
| 34 | menu_host.Background(make_brush(theme.card_background)); | src/WinBlaze.UI/src/MainWindow.cpp:997 | bounded-color-resources |
| 35 | progress_track.Background(make_brush(theme.progress_track)); | src/WinBlaze.UI/src/MainWindow.cpp:1182 | bounded-color-resources |
| 36 | m_scan_progress_fill.Background(make_brush(theme.progress_fill)); | src/WinBlaze.UI/src/MainWindow.cpp:1189 | bounded-color-resources |
| 37 | banner.Background(make_brush(background)); | src/WinBlaze.UI/src/MainWindow.cpp:1456 | bounded-color-resources |
| 38 | banner.BorderBrush(make_brush(border)); | src/WinBlaze.UI/src/MainWindow.cpp:1457 | bounded-color-resources |
| 39 | apply_card_style(scan_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1253 | bounded-color-resources |
| 40 | apply_card_style(scan_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1253 | bounded-color-resources |
| 41 | apply_card_style(summary_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1522 | bounded-color-resources |
| 42 | apply_card_style(summary_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1522 | bounded-color-resources |
| 43 | apply_card_style(search_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1550 | bounded-color-resources |
| 44 | apply_card_style(search_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1550 | bounded-color-resources |
| 45 | apply_card_style(tree_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1746 | bounded-color-resources |
| 46 | apply_card_style(tree_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1746 | bounded-color-resources |
| 47 | apply_card_style(extension_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1864 | bounded-color-resources |
| 48 | apply_card_style(extension_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1864 | bounded-color-resources |
| 49 | apply_card_style(detail_card); — shared background brush | src/WinBlaze.UI/src/MainWindow.cpp:1922 | bounded-color-resources |
| 50 | apply_card_style(detail_card); — shared border brush | src/WinBlaze.UI/src/MainWindow.cpp:1922 | bounded-color-resources |
