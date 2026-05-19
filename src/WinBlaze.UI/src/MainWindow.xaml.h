#pragma once

#include "pch.h"
#include "NativeBridge.h"

#include <map>
#include <unordered_set>

namespace winrt::WinBlaze::UI::implementation
{
    enum class ShellSection
    {
        Overview,
        Tree,
        Treemap,
        Search,
        Diagnostics,
    };

    struct MainWindow : Microsoft::UI::Xaml::WindowT<MainWindow>
    {
        MainWindow();

        void OnWindowLoaded(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnWindowClosed(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::WindowEventArgs const&);
        void OnCompositionRendering(winrt::Windows::Foundation::IInspectable const&, winrt::Windows::Foundation::IInspectable const&);
        void OnWindowKeyDown(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const&);
        void OnStartClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnIncrementalScanClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void BeginScanFromCurrentRoot(bool incremental = false);
        void OnCancelClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnStartTapped(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&);
        void OnSearchClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnSearchQueryChanged(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::Controls::TextChangedEventArgs const&);
        void OnSearchOptionsChanged(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void OnSearchResultClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnDeveloperDiagnosticsToggled(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnOptionalPanelToggleClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnSectionMenuSelectionChanged(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void OnBreadcrumbClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnTreeItemClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnTreeSnapshotExpandClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnTreeWindowPreviousClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnTreeWindowNextClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnTreeSelectionChanged(
            winrt::Windows::Foundation::IInspectable const&,
            Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void OnTreemapSurfaceSizeChanged(
            winrt::Windows::Foundation::IInspectable const&,
            Microsoft::UI::Xaml::SizeChangedEventArgs const&);
        void OnTreemapSurfacePointerMoved(
            winrt::Windows::Foundation::IInspectable const&,
            Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnTreemapSurfacePointerExited(
            winrt::Windows::Foundation::IInspectable const&,
            Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnTreemapSurfaceTapped(
            winrt::Windows::Foundation::IInspectable const&,
            Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&);

        void SetSection(ShellSection section);
        void ApplyShellState();
        void ScheduleUiFlush();
        void FlushPendingUiState();
        void ScheduleTreemapRender(std::wstring const& reason);
        void UpdatePerformanceCounters(std::wstring const& reason);
        void UpdateSummaryText();
        void UpdateRuntimeSnapshot();
        void UpdateCorrectnessDiagnostics();
        void UpdateRecentIssueDiagnostics();
        void UpdateCatalogSnapshot();
        void UpdateBreadcrumbs();
        void UpdateStatus(std::wstring const& text);
        void UpdateEventText(std::wstring const& text);
        void UpdateSearchPreview(std::wstring const& text);
        void RefreshInstantSearch();
        void LoadPersistedCatalogSnapshot();
        void UpdateProgress(double percent, std::wstring const& text);
        void HandleNativeEvent(WbEvent const& event);
        std::wstring FormatSummary(WbEvent const& event) const;
        std::wstring FormatSearchQuery();
        std::wstring SectionName(ShellSection section) const;
        std::wstring ComboBoxSelectionText(
            Microsoft::UI::Xaml::Controls::ComboBox const& box,
            wchar_t const* fallback) const;
        std::wstring CurrentVisualizationLabel() const;
        void SelectVisualizationTarget(
            std::wstring const& name,
            std::wstring const& path,
            std::wstring const& kind,
            std::wstring const& size_text);
        void UpdateTreemapFocus(
            std::wstring const& name,
            std::wstring const& path,
            std::wstring const& kind,
            std::wstring const& size_text);
        void ApplyTreemapColorRules(
            std::wstring const& kind,
            Microsoft::UI::Xaml::Controls::Border const& panel);
        void SetControlVisibility(Microsoft::UI::Xaml::FrameworkElement const& control, bool visible);
        void FocusSearchBox();
        void FocusRootPathBox();
        void NavigateToSection(ShellSection section);
        void UpdateNavigationChips();
        int SectionMenuIndex(ShellSection section) const;
        ShellSection SectionFromMenuIndex(int index) const;
        Microsoft::UI::Xaml::Media::SolidColorBrush MakeBrush(Windows::UI::Color const& color) const;
        void ApplyCardStyle(Microsoft::UI::Xaml::Controls::Border const& card) const;
        void ApplyCompactCardStyle(Microsoft::UI::Xaml::Controls::Border const& card) const;
        void ApplyAccentPanelStyle(
            Microsoft::UI::Xaml::Controls::Border const& panel,
            Windows::UI::Color const& background,
            Windows::UI::Color const& border) const;
        Microsoft::UI::Xaml::Controls::TextBlock MakeCardTitle(std::wstring_view text) const;
        void ApplyNavigationChipStyle(Microsoft::UI::Xaml::Controls::Button const& chip, bool active) const;

    private:
        struct TreeCatalogEntry
        {
            std::wstring name;
            std::wstring path;
            std::wstring kind;
            std::wstring size_text;
            uint64_t size_bytes{};
            int progress{};
            std::optional<int64_t> modified_utc;
            std::wstring description;
            std::wstring search_text_lower;
            std::wstring extension_lower;
            int path_depth{};
            std::wstring parent_path;
            std::wstring top_group;
        };

        struct TreemapTileLayout
        {
            float left{};
            float top{};
            float right{};
            float bottom{};
            std::wstring name;
            std::wstring path;
            std::wstring kind;
            std::wstring size_text;
        };

        void UpdateTreeSnapshotPreview(std::vector<TreeCatalogEntry> const& entries);
        void UpdateSearchResultsPreview(std::vector<TreeCatalogEntry> const& hits);

        void BuildShell();
        Microsoft::UI::Xaml::Controls::ListViewItem CreateTreeListItem(TreeCatalogEntry const& entry) const;
        void PopulateTreeList(std::vector<TreeCatalogEntry> const& entries);
        std::vector<TreeCatalogEntry> FilterTreeCatalog() const;
        bool MatchesInstantSearch(TreeCatalogEntry const& entry) const;
        std::wstring TreeCatalogKey(TreeCatalogEntry const& entry) const;
        static std::wstring Utf8ToWide(std::string_view text);
        TreeCatalogEntry CatalogEntryFromNative(WbCatalogEntry const& entry) const;
        void RenderTreemapProbeFrame(int width, int height);

        Microsoft::UI::Xaml::Controls::TextBlock StatusText() const { return m_status_text; }
        Microsoft::UI::Xaml::Controls::TextBlock SectionText() const { return m_section_text; }
        Microsoft::UI::Xaml::Controls::Button OverviewBreadcrumbButton() const { return m_overview_breadcrumb_button; }
        Microsoft::UI::Xaml::Controls::Button ScanRootButton() const { return m_scan_root_button; }
        Microsoft::UI::Xaml::Controls::TextBlock RootBreadcrumbText() const { return m_root_breadcrumb_text; }
        Microsoft::UI::Xaml::Controls::TextBox RootPathBox() const { return m_root_path_box; }
        Microsoft::UI::Xaml::Controls::Button StartScanButton() const { return m_start_scan_button; }
        Microsoft::UI::Xaml::Controls::Button IncrementalScanButton() const { return m_incremental_scan_button; }
        Microsoft::UI::Xaml::Controls::Button CancelScanButton() const { return m_cancel_scan_button; }
        Microsoft::UI::Xaml::Controls::Border ScanProgressFill() const { return m_scan_progress_fill; }
        Microsoft::UI::Xaml::Controls::TextBlock ProgressText() const { return m_progress_text; }
        Microsoft::UI::Xaml::Controls::Border LoadingBanner() const { return m_loading_banner; }
        Microsoft::UI::Xaml::Controls::Border ScanningBanner() const { return m_scanning_banner; }
        Microsoft::UI::Xaml::Controls::Border EmptyBanner() const { return m_empty_banner; }
        Microsoft::UI::Xaml::Controls::Border ErrorBanner() const { return m_error_banner; }
        Microsoft::UI::Xaml::Controls::TextBlock ErrorText() const { return m_error_text; }
        Microsoft::UI::Xaml::Controls::TextBlock EventText() const { return m_event_text; }
        Microsoft::UI::Xaml::Controls::TextBlock SummaryText() const { return m_summary_text; }
        Microsoft::UI::Xaml::Controls::TextBlock RuntimeSnapshotText() const { return m_runtime_snapshot_text; }
        Microsoft::UI::Xaml::Controls::ComboBox SectionMenu() const { return m_section_menu; }
        Microsoft::UI::Xaml::Controls::CheckBox CurrentStateToggle() const { return m_current_state_toggle; }
        Microsoft::UI::Xaml::Controls::CheckBox FolderViewToggle() const { return m_folder_view_toggle; }
        Microsoft::UI::Xaml::Controls::CheckBox FolderTreeToggle() const { return m_folder_tree_toggle; }
        Microsoft::UI::Xaml::Controls::CheckBox DeveloperDiagnosticsToggle() const { return m_developer_diagnostics_toggle; }
        Microsoft::UI::Xaml::Controls::StackPanel DeveloperDiagnosticsPanel() const { return m_developer_diagnostics_panel; }
        Microsoft::UI::Xaml::Controls::TextBlock CorrectnessText() const { return m_correctness_text; }
        Microsoft::UI::Xaml::Controls::TextBlock RecentIssuesText() const { return m_recent_issues_text; }
        Microsoft::UI::Xaml::Controls::TextBlock IssueDrilldownText() const { return m_issue_drilldown_text; }
        Microsoft::UI::Xaml::Controls::TextBlock CatalogSnapshotText() const { return m_catalog_snapshot_text; }
        Microsoft::UI::Xaml::Controls::Border OverviewCard() const { return m_overview_card; }
        Microsoft::UI::Xaml::Controls::Border TreeCard() const { return m_tree_card; }
        Microsoft::UI::Xaml::Controls::Button TreeSnapshotExpandButton() const { return m_tree_snapshot_expand_button; }
        Microsoft::UI::Xaml::Controls::Button TreeWindowPreviousButton() const { return m_tree_window_previous_button; }
        Microsoft::UI::Xaml::Controls::Button TreeWindowNextButton() const { return m_tree_window_next_button; }
        Microsoft::UI::Xaml::Controls::StackPanel TreeSnapshotPanel() const { return m_tree_snapshot_panel; }
        Microsoft::UI::Xaml::Controls::StackPanel TreeSnapshotExtraPanel() const { return m_tree_snapshot_extra_panel; }
        Microsoft::UI::Xaml::Controls::TextBlock TreeListStatusText() const { return m_tree_list_status_text; }
        Microsoft::UI::Xaml::Controls::Border SearchCard() const { return m_search_card; }
        Microsoft::UI::Xaml::Controls::StackPanel SearchResultsPanel() const { return m_search_results_panel; }
        Microsoft::UI::Xaml::Controls::Border DiagnosticsCard() const { return m_diagnostics_card; }
        Microsoft::UI::Xaml::Controls::Border TreemapCard() const { return m_treemap_card; }
        Microsoft::UI::Xaml::Controls::Border DetailCard() const { return m_detail_card; }
        Microsoft::UI::Xaml::Controls::ListView TreeListView() const { return m_tree_list_view; }
        Microsoft::UI::Xaml::Controls::TextBox SearchBox() const { return m_search_box; }
        Microsoft::UI::Xaml::Controls::TextBox ExtensionBox() const { return m_extension_box; }
        Microsoft::UI::Xaml::Controls::TextBox MinSizeBox() const { return m_min_size_box; }
        Microsoft::UI::Xaml::Controls::TextBox ModifiedAfterBox() const { return m_modified_after_box; }
        Microsoft::UI::Xaml::Controls::TextBox ModifiedBeforeBox() const { return m_modified_before_box; }
        Microsoft::UI::Xaml::Controls::ComboBox MatchModeBox() const { return m_match_mode_box; }
        Microsoft::UI::Xaml::Controls::ComboBox SortFieldBox() const { return m_sort_field_box; }
        Microsoft::UI::Xaml::Controls::Button SearchApplyButton() const { return m_search_apply_button; }
        Microsoft::UI::Xaml::Controls::ComboBox SortDirectionBox() const { return m_sort_direction_box; }
        Microsoft::UI::Xaml::Controls::TextBlock SearchPreviewText() const { return m_search_preview_text; }
        Microsoft::UI::Xaml::Controls::TextBlock PerformanceText() const { return m_performance_text; }
        Microsoft::UI::Xaml::Controls::SwapChainPanel TreemapSurface() const { return m_treemap_surface; }
        Microsoft::UI::Xaml::Controls::TextBlock TreemapSurfaceStatusText() const { return m_treemap_surface_status_text; }
        Microsoft::UI::Xaml::Controls::Border TreemapZoomOverlay() const { return m_treemap_zoom_overlay; }
        Microsoft::UI::Xaml::Controls::TextBlock TreemapZoomTitle() const { return m_treemap_zoom_title; }
        Microsoft::UI::Xaml::Controls::TextBlock TreemapZoomDescription() const { return m_treemap_zoom_description; }
        Microsoft::UI::Xaml::Controls::TextBlock SelectionText() const { return m_selection_text; }
        Microsoft::UI::Xaml::Controls::TextBlock SelectionSizeText() const { return m_selection_size_text; }
        Microsoft::UI::Xaml::Controls::Border VolumeDetailPanel() const { return m_volume_detail_panel; }
        Microsoft::UI::Xaml::Controls::Border FolderDetailPanel() const { return m_folder_detail_panel; }
        Microsoft::UI::Xaml::Controls::Border FileDetailPanel() const { return m_file_detail_panel; }

        Microsoft::UI::Xaml::Controls::TextBlock m_status_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_section_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_overview_breadcrumb_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_scan_root_button{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_root_breadcrumb_text{ nullptr };
        Microsoft::UI::Xaml::Controls::StackPanel m_path_breadcrumb_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_root_path_box{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_start_scan_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_incremental_scan_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_cancel_scan_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_scan_progress_fill{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_progress_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_loading_banner{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_scanning_banner{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_empty_banner{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_error_banner{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_error_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_event_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_summary_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_runtime_snapshot_text{ nullptr };
        Microsoft::UI::Xaml::Controls::ComboBox m_section_menu{ nullptr };
        Microsoft::UI::Xaml::Controls::CheckBox m_current_state_toggle{ nullptr };
        Microsoft::UI::Xaml::Controls::CheckBox m_folder_view_toggle{ nullptr };
        Microsoft::UI::Xaml::Controls::CheckBox m_folder_tree_toggle{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_correctness_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_recent_issues_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_issue_drilldown_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_catalog_snapshot_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_overview_card{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_tree_card{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_tree_snapshot_expand_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_tree_window_previous_button{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_tree_window_next_button{ nullptr };
        Microsoft::UI::Xaml::Controls::StackPanel m_tree_snapshot_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::StackPanel m_tree_snapshot_extra_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_tree_list_status_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_search_card{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_diagnostics_card{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_treemap_card{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_detail_card{ nullptr };
        std::vector<Microsoft::UI::Xaml::Controls::Button> m_nav_chips;
        Microsoft::UI::Xaml::Controls::ListView m_tree_list_view{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_search_box{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_extension_box{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_min_size_box{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_modified_after_box{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBox m_modified_before_box{ nullptr };
        Microsoft::UI::Xaml::Controls::ComboBox m_match_mode_box{ nullptr };
        Microsoft::UI::Xaml::Controls::ComboBox m_sort_field_box{ nullptr };
        Microsoft::UI::Xaml::Controls::Button m_search_apply_button{ nullptr };
        Microsoft::UI::Xaml::Controls::ComboBox m_sort_direction_box{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_search_preview_text{ nullptr };
        Microsoft::UI::Xaml::Controls::StackPanel m_search_results_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::CheckBox m_developer_diagnostics_toggle{ nullptr };
        Microsoft::UI::Xaml::Controls::StackPanel m_developer_diagnostics_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_performance_text{ nullptr };
        Microsoft::UI::Xaml::Controls::SwapChainPanel m_treemap_surface{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_treemap_surface_status_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_treemap_zoom_overlay{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_treemap_zoom_title{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_treemap_zoom_description{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_selection_text{ nullptr };
        Microsoft::UI::Xaml::Controls::TextBlock m_selection_size_text{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_volume_detail_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_folder_detail_panel{ nullptr };
        Microsoft::UI::Xaml::Controls::Border m_file_detail_panel{ nullptr };
        std::vector<TreeCatalogEntry> m_tree_catalog;
        std::unordered_set<std::wstring> m_tree_catalog_keys;
        std::vector<TreeCatalogEntry> m_instant_search_hits;
        size_t m_tree_window_offset{ 0 };
        bool m_tree_updates_ready{ false };
        bool m_tree_selection_updates_suppressed{ false };

        Microsoft::UI::Dispatching::DispatcherQueueTimer m_ui_flush_timer{ nullptr };
        Microsoft::UI::Dispatching::DispatcherQueueTimer m_treemap_render_timer{ nullptr };
        std::mutex m_pending_ui_mutex;
        bool m_ui_flush_requested{ false };
        bool m_treemap_render_requested{ false };
        bool m_shell_ready{ false };
        bool m_session_active{ false };
        bool m_has_results{ false };
        bool m_has_error{ false };
        bool m_show_current_state{ false };
        bool m_show_folder_view{ false };
        bool m_show_folder_tree{ false };
        bool m_section_menu_updates_suppressed{ false };
        ShellSection m_active_section{ ShellSection::Overview };
        std::chrono::steady_clock::time_point m_scan_started_at{};
        std::chrono::duration<double, std::milli> m_last_scan_duration{ 0.0 };
        std::wstring m_last_scan_duration_text{ L"Scan duration: n/a" };
        std::wstring m_last_cache_load_text{ L"Cache load: n/a" };
        uint64_t m_scan_issue_count{ 0 };
        std::wstring m_last_scan_issue_text{ L"none" };
        std::vector<std::wstring> m_recent_scan_issues;
        std::wstring m_current_root_path{ L"C:\\" };
        std::wstring m_current_selection_name{ L"Root volume" };
        std::wstring m_current_selection_path{ L"C:\\" };
        std::wstring m_current_selection_kind{ L"Volume" };
        std::wstring m_current_selection_size{ L"0 B" };
        std::wstring m_hovered_treemap_name;
        std::wstring m_hovered_treemap_path;
        std::wstring m_hovered_treemap_kind;
        std::wstring m_hovered_treemap_size;
        std::wstring m_treemap_render_status{ L"Direct2D/D3D render stack has not been probed." };
        bool m_treemap_probe_frame_rendered{ false };
        bool m_treemap_render_dirty{ true };
        int m_treemap_render_width{ 0 };
        int m_treemap_render_height{ 0 };
        uint64_t m_total_treemap_render_request_count{ 0 };
        uint64_t m_total_treemap_render_flush_count{ 0 };
        uint64_t m_total_treemap_render_coalesced_count{ 0 };
        std::vector<TreemapTileLayout> m_treemap_tile_layout;
        ::WinBlaze::UI::NativeBridge::SessionHandle m_session{};

        struct PendingUiState
        {
            bool status_dirty{ false };
            std::wstring status_text;
            bool event_dirty{ false };
            std::wstring event_text;
            bool summary_dirty{ false };
            std::wstring summary_text;
            bool progress_dirty{ false };
            double progress_percent{ 0.0 };
            std::wstring progress_text;
            bool error_dirty{ false };
            std::wstring error_text;
            bool selection_dirty{ false };
            std::wstring selected_name;
            std::wstring selected_path;
            std::wstring selected_kind;
            std::wstring selected_size;
            bool visualization_dirty{ false };
            std::wstring treemap_hover_name;
            std::wstring treemap_hover_path;
            std::wstring treemap_hover_kind;
            std::wstring treemap_hover_size;
            bool catalog_dirty{ false };
            std::vector<TreeCatalogEntry> catalog_entries;
            bool reload_snapshot{ false };
            bool diagnostics_dirty{ false };
            bool correctness_dirty{ false };
            bool reset_scan_issues{ false };
            uint64_t issue_count_delta{ 0 };
            std::map<uint32_t, uint64_t> issue_code_deltas;
            std::wstring last_issue_text;
            std::vector<std::wstring> recent_issue_texts;
            bool incremental_changes_dirty{ false };
            uint64_t incremental_added{ 0 };
            uint64_t incremental_removed{ 0 };
            uint64_t incremental_modified{ 0 };
            uint64_t incremental_renamed{ 0 };
            uint64_t incremental_moved{ 0 };
            uint64_t progress_items_done{ 0 };
            uint64_t progress_items_total{ 0 };
            uint64_t progress_bytes_done{ 0 };
            uint64_t progress_bytes_total{ 0 };
            uint64_t summary_files_seen{ 0 };
            uint64_t summary_directories_seen{ 0 };
            uint64_t summary_total_size_bytes{ 0 };
        };

        PendingUiState m_pending_ui_state{};
        std::chrono::steady_clock::time_point m_last_ui_event_time{};
        std::chrono::steady_clock::time_point m_last_ui_flush_time{};
        double m_last_ui_latency_ms{ 0.0 };
        double m_last_input_latency_ms{ 0.0 };
        double m_last_ui_flush_duration_ms{ 0.0 };
        double m_peak_ui_flush_duration_ms{ 0.0 };
        double m_last_composition_frame_ms{ 0.0 };
        double m_peak_composition_frame_ms{ 0.0 };
        std::chrono::steady_clock::time_point m_last_composition_frame_time{};
        winrt::event_token m_composition_rendering_token{};
        bool m_composition_rendering_registered{ false };
        uint64_t m_last_progress_items_done{ 0 };
        uint64_t m_last_progress_items_total{ 0 };
        uint64_t m_last_progress_bytes_done{ 0 };
        uint64_t m_last_progress_bytes_total{ 0 };
        uint64_t m_last_summary_files_seen{ 0 };
        uint64_t m_last_summary_directories_seen{ 0 };
        uint64_t m_last_summary_total_size_bytes{ 0 };
        uint64_t m_incremental_added{ 0 };
        uint64_t m_incremental_removed{ 0 };
        uint64_t m_incremental_modified{ 0 };
        uint64_t m_incremental_renamed{ 0 };
        uint64_t m_incremental_moved{ 0 };
        std::map<uint32_t, uint64_t> m_scan_issue_code_counts;
        uint64_t m_last_working_set_bytes{ 0 };
        uint64_t m_peak_working_set_bytes{ 0 };
        uint64_t m_total_ui_flush_count{ 0 };
        uint64_t m_total_composition_frame_count{ 0 };
        uint64_t m_pending_event_count{ 0 };
    };
}
