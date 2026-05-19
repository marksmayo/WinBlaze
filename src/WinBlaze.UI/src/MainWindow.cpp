#include "pch.h"
#include "NativeBridge.h"
#include "MainWindow.xaml.h"
#include "ShellTheme.h"
#include "StartupTrace.h"

namespace
{
    constexpr int64_t kFileTimeTicksPerSecond = 10'000'000LL;
    constexpr int64_t kUnixToFileTimeSeconds = 11'644'473'600LL;
    constexpr size_t kTreeListVirtualizedWindowLimit = 256;
    constexpr size_t kCatalogSnapshotLoadLimit = 8192;
    constexpr size_t kRecentIssueLimit = 6;

    std::wstring FirstTextBlockText(winrt::Windows::Foundation::IInspectable const& value)
    {
        if (!value) {
            return {};
        }

        if (auto text = value.try_as<winrt::Microsoft::UI::Xaml::Controls::TextBlock>()) {
            return text.Text().c_str();
        }

        if (auto content = value.try_as<winrt::Microsoft::UI::Xaml::Controls::ContentControl>()) {
            if (auto text = FirstTextBlockText(content.Content()); !text.empty()) {
                return text;
            }
        }

        if (auto panel = value.try_as<winrt::Microsoft::UI::Xaml::Controls::Panel>()) {
            for (auto const& child : panel.Children()) {
                if (auto text = FirstTextBlockText(child); !text.empty()) {
                    return text;
                }
            }
        }

        return winrt::unbox_value_or<winrt::hstring>(value, winrt::hstring{}).c_str();
    }

    winrt::Microsoft::UI::Xaml::CornerRadius UniformRadius(double value)
    {
        return winrt::Microsoft::UI::Xaml::CornerRadius{ value, value, value, value };
    }

    winrt::Microsoft::UI::Xaml::Thickness UniformThickness(double value)
    {
        return winrt::Microsoft::UI::Xaml::Thickness{ value, value, value, value };
    }

    std::wstring TrimCopy(std::wstring text)
    {
        const auto first = text.find_first_not_of(L" \t\r\n");
        if (first == std::wstring::npos) {
            return {};
        }
        const auto last = text.find_last_not_of(L" \t\r\n");
        return text.substr(first, last - first + 1);
    }

    std::wstring LowercaseCopy(std::wstring text)
    {
        std::transform(text.begin(), text.end(), text.begin(), [](wchar_t ch) {
            return static_cast<wchar_t>(std::towlower(ch));
        });
        return text;
    }

    std::wstring ExtensionLower(std::wstring const& path)
    {
        const auto dot = path.find_last_of(L'.');
        const auto slash = path.find_last_of(L"\\/");
        if (dot == std::wstring::npos || (slash != std::wstring::npos && dot < slash)) {
            return {};
        }
        return LowercaseCopy(path.substr(dot + 1));
    }

    int PathDepth(std::wstring const& path)
    {
        if (path.size() <= 3) {
            return 0;
        }
        return static_cast<int>(std::count(path.begin() + 3, path.end(), L'\\'));
    }

    std::wstring ParentPath(std::wstring const& path)
    {
        if (path.size() <= 3) {
            return {};
        }
        const auto trimmed_end = path.find_last_not_of(L"\\/");
        if (trimmed_end == std::wstring::npos || trimmed_end < 3) {
            return {};
        }
        const auto slash = path.find_last_of(L"\\/", trimmed_end);
        if (slash == std::wstring::npos || slash < 2) {
            return {};
        }
        if (slash == 2 && path.size() >= 3 && path[1] == L':') {
            return path.substr(0, 3);
        }
        return path.substr(0, slash);
    }

    std::wstring TopLevelPathGroup(std::wstring const& path)
    {
        if (path.size() >= 3 && path[1] == L':' && (path[2] == L'\\' || path[2] == L'/')) {
            const auto next = path.find_first_of(L"\\/", 3);
            if (next == std::wstring::npos) {
                return path.substr(0, 3);
            }
            return path.substr(0, next);
        }

        const auto first = path.find_first_not_of(L"\\/");
        if (first == std::wstring::npos) {
            return {};
        }
        const auto next = path.find_first_of(L"\\/", first);
        return path.substr(first, next == std::wstring::npos ? std::wstring::npos : next - first);
    }

    std::optional<int64_t> ParseUtcDateBoundary(std::wstring const& text)
    {
        const std::wstring trimmed = TrimCopy(text);
        if (trimmed.empty()) {
            return std::nullopt;
        }

        int year = 0;
        int month = 0;
        int day = 0;
        wchar_t dash1 = L'\0';
        wchar_t dash2 = L'\0';
        std::wistringstream stream(trimmed);
        if (!(stream >> year >> dash1 >> month >> dash2 >> day) || dash1 != L'-' || dash2 != L'-') {
            return std::nullopt;
        }
        if (year < 1601 || month < 1 || month > 12 || day < 1 || day > 31) {
            return std::nullopt;
        }

        std::tm tm{};
        tm.tm_year = year - 1900;
        tm.tm_mon = month - 1;
        tm.tm_mday = day;
        tm.tm_isdst = 0;

        const auto unix_seconds = _mkgmtime64(&tm);
        if (unix_seconds < 0) {
            return std::nullopt;
        }

        return (unix_seconds + kUnixToFileTimeSeconds) * kFileTimeTicksPerSecond;
    }

    std::optional<uint64_t> ParseSizeTextBytes(std::wstring text)
    {
        text = TrimCopy(std::move(text));
        if (text.empty()) {
            return std::nullopt;
        }

        text.erase(std::remove(text.begin(), text.end(), L','), text.end());
        std::transform(text.begin(), text.end(), text.begin(), [](wchar_t ch) {
            return static_cast<wchar_t>(std::towlower(ch));
        });

        std::wistringstream stream(text);
        long double value = 0.0;
        std::wstring unit;
        stream >> value >> unit;
        if (!stream || value < 0.0) {
            return std::nullopt;
        }

        long double scale = 1.0;
        if (unit == L"t" || unit == L"tb" || unit == L"tib") {
            scale = 1024.0L * 1024.0L * 1024.0L * 1024.0L;
        } else if (unit == L"g" || unit == L"gb" || unit == L"gib") {
            scale = 1024.0L * 1024.0L * 1024.0L;
        } else if (unit == L"m" || unit == L"mb" || unit == L"mib") {
            scale = 1024.0L * 1024.0L;
        } else if (unit == L"k" || unit == L"kb" || unit == L"kib") {
            scale = 1024.0L;
        } else if (!unit.empty() && unit != L"b" && unit != L"byte" && unit != L"bytes") {
            return std::nullopt;
        }

        const long double bytes = value * scale;
        if (bytes > static_cast<long double>(UINT64_MAX)) {
            return UINT64_MAX;
        }
        return static_cast<uint64_t>(bytes);
    }

    std::wstring FormatBytes(uint64_t size)
    {
        constexpr double KB = 1024.0;
        constexpr double MB = KB * 1024.0;
        constexpr double GB = MB * 1024.0;
        const double value = static_cast<double>(size);
        std::wostringstream stream;
        stream.setf(std::ios::fixed);
        stream.precision(1);
        if (value >= GB) {
            stream << (value / GB) << L" GB";
        } else if (value >= MB) {
            stream << (value / MB) << L" MB";
        } else if (value >= KB) {
            stream << (value / KB) << L" KB";
        } else {
            stream.precision(0);
            stream << value << L" B";
        }
        return stream.str();
    }

    std::wstring IssueCodeLabel(uint32_t code)
    {
        switch (code) {
        case 10:
            return L"Permission denied";
        case 11:
            return L"Not found";
        case 12:
            return L"Sharing violation";
        case 13:
            return L"Transient I/O";
        case 14:
            return L"Unsupported filesystem";
        case 15:
            return L"Unknown";
        default:
            return L"Native error";
        }
    }

    uint64_t CurrentWorkingSetBytes()
    {
        PROCESS_MEMORY_COUNTERS_EX counters{};
        counters.cb = sizeof(counters);
        if (::GetProcessMemoryInfo(
                ::GetCurrentProcess(),
                reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&counters),
                sizeof(counters))) {
            return static_cast<uint64_t>(counters.WorkingSetSize);
        }
        return 0;
    }

    std::wstring HresultText(HRESULT result)
    {
        wchar_t buffer[16]{};
        swprintf_s(buffer, L"0x%08X", static_cast<unsigned int>(result));
        return buffer;
    }

    std::wstring ProbeTreemapRenderStack()
    {
        winrt::com_ptr<ID3D11Device> d3d_device;
        winrt::com_ptr<ID3D11DeviceContext> d3d_context;
        D3D_FEATURE_LEVEL selected_level{};
        constexpr D3D_FEATURE_LEVEL levels[] = {
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
            D3D_FEATURE_LEVEL_10_1,
            D3D_FEATURE_LEVEL_10_0,
        };

        HRESULT result = ::D3D11CreateDevice(
            nullptr,
            D3D_DRIVER_TYPE_HARDWARE,
            nullptr,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            levels,
            ARRAYSIZE(levels),
            D3D11_SDK_VERSION,
            d3d_device.put(),
            &selected_level,
            d3d_context.put());
        if (FAILED(result)) {
            return L"Direct3D hardware device probe failed: " + HresultText(result);
        }

        winrt::com_ptr<IDXGIDevice> dxgi_device;
        result = d3d_device->QueryInterface(__uuidof(IDXGIDevice), dxgi_device.put_void());
        if (FAILED(result)) {
            return L"DXGI device probe failed: " + HresultText(result);
        }

        winrt::com_ptr<ID2D1Factory3> d2d_factory;
        D2D1_FACTORY_OPTIONS options{};
        result = ::D2D1CreateFactory(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            __uuidof(ID2D1Factory3),
            &options,
            reinterpret_cast<void**>(d2d_factory.put()));
        if (FAILED(result)) {
            return L"Direct2D factory probe failed: " + HresultText(result);
        }

        winrt::com_ptr<ID2D1Device> d2d_device;
        result = d2d_factory->CreateDevice(dxgi_device.get(), d2d_device.put());
        if (FAILED(result)) {
            return L"Direct2D device probe failed: " + HresultText(result);
        }

        const int level_major = static_cast<int>((selected_level >> 12) & 0xF);
        const int level_minor = static_cast<int>((selected_level >> 8) & 0xF);
        return L"Direct2D/D3D render stack ready: D3D feature level " +
            std::to_wstring(level_major) + L"." + std::to_wstring(level_minor) +
            L"; catalog drawing will start after the treemap surface is laid out.";
    }
}

namespace winrt::WinBlaze::UI::implementation
{
    MainWindow::MainWindow()
    {
        TraceStartup(L"MainWindow::MainWindow begin");
        try {
            Closed({ this, &MainWindow::OnWindowClosed });
            BuildShell();
            TraceStartup(L"MainWindow after BuildShell");
            TraceStartup(L"MainWindow deferring NativeBridge initialize until first scan or cache load");
            TraceStartup(L"MainWindow using dispatcher-driven UI flush scheduling");

            TraceStartup(L"MainWindow using stable shell navigation chips");

            if (RootPathBox()) {
                m_current_root_path = RootPathBox().Text().c_str();
                TraceStartup(L"MainWindow after root path");
            } else {
                TraceStartup(L"MainWindow skipped root path box");
            }
            UpdateStatus(L"Idle");
            UpdateEventText(L"Ready to scan.");
            UpdateSearchPreview(FormatSearchQuery());
            UpdateProgress(0.0, L"0% complete");
            RefreshInstantSearch();
            TraceStartup(L"MainWindow after refresh instant search");
            TraceStartup(L"MainWindow after tree select");
            SelectVisualizationTarget(L"Root volume", m_current_root_path, L"Volume", L"0 B");
            TraceStartup(L"MainWindow after select visualization target");
            UpdatePerformanceCounters(L"startup");
            TraceStartup(L"MainWindow after performance counters");
            SetSection(ShellSection::Overview);
            TraceStartup(L"MainWindow after set section");
            ApplyShellState();
            TraceStartup(L"MainWindow after first apply shell state");
            UpdateSummaryText();
            TraceStartup(L"MainWindow after summary text");
            UpdateRuntimeSnapshot();
            TraceStartup(L"MainWindow after runtime snapshot");
            UpdateCatalogSnapshot();
            TraceStartup(L"MainWindow after snapshot updates");
            m_shell_ready = true;
            ApplyShellState();
            TraceStartup(L"MainWindow after shell ready apply");
            TraceStartup(L"MainWindow auto scan skipped; waiting for user-selected root");
            TraceStartup(L"MainWindow constructor complete");
        }
        catch (winrt::hresult_error const& error) {
            std::wstring message = L"MainWindow startup failed: ";
            message += L"[";
            message += winrt::to_hstring(static_cast<uint32_t>(error.code())).c_str();
            message += L"] ";
            message += error.message().c_str();
            TraceStartup(message);
            ReportFailure(L"MainWindow startup", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
        catch (std::exception const& error) {
            std::wstring message = L"MainWindow startup failed: ";
            message += winrt::to_hstring(error.what()).c_str();
            TraceStartup(message);
            ReportFailure(L"MainWindow startup", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
        catch (...) {
            TraceStartup(L"MainWindow startup failed: unknown exception");
            ReportFailure(L"MainWindow startup", L"MainWindow startup failed: unknown exception");
            ::MessageBoxW(nullptr, L"MainWindow startup failed: unknown exception", L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
    }

    void MainWindow::BuildShell()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        using namespace Microsoft::UI::Xaml::Media;

        TraceStartup(L"BuildShell begin");

        auto make_brush = [this](Windows::UI::Color const& color) {
            return MakeBrush(color);
        };
        auto apply_card_style = [this](Border const& card) {
            ApplyCardStyle(card);
        };
        auto apply_compact_card_style = [this](Border const& card) {
            ApplyCompactCardStyle(card);
        };
        auto make_card_title = [this](std::wstring_view text) {
            return MakeCardTitle(text);
        };
        TraceStartup(L"BuildShell after brush helper");

        auto const& theme = ActiveShellTheme();
        auto root = Grid{};
        root.RequestedTheme(ElementTheme::Dark);
        root.Background(make_brush(theme.app_background));
        root.KeyDown({ this, &MainWindow::OnWindowKeyDown });
        root.Loaded({ this, &MainWindow::OnWindowLoaded });
        TraceStartup(L"BuildShell after root grid");

        auto shell = StackPanel{};
        shell.Margin(Thickness{ 20.0, 18.0, 20.0, 20.0 });
        shell.Spacing(10);
        TraceStartup(L"BuildShell after shell stack");

        auto title = TextBlock{};
        title.Text(L"WinBlaze");
        title.FontSize(26);
        title.Foreground(make_brush(theme.text_primary));
        shell.Children().Append(title);
        TraceStartup(L"BuildShell after title");

        auto subtitle = TextBlock{};
        subtitle.Text(L"Live scan controls, indexed search, optional folder panels, and treemap are active.");
        subtitle.FontSize(15);
        subtitle.Foreground(make_brush(theme.text_primary));
        subtitle.TextWrapping(TextWrapping::WrapWholeWords);
        shell.Children().Append(subtitle);
        TraceStartup(L"BuildShell after subtitle");

        TraceStartup(L"BuildShell skipped restoration banner");

        auto status_row = StackPanel{};
        status_row.Orientation(Orientation::Horizontal);
        status_row.Spacing(12);

        auto status_chip = Border{};
        status_chip.CornerRadius(UniformRadius(theme.chip_radius));
        status_chip.Padding(Thickness{ 12.0, 6.0, 12.0, 6.0 });
        status_chip.Background(make_brush(theme.chip_background));
        m_status_text = TextBlock{};
        m_status_text.Text(L"Idle");
        m_status_text.Foreground(make_brush(theme.text_primary));
        status_chip.Child(m_status_text);
        status_row.Children().Append(status_chip);

        auto section_chip = Border{};
        section_chip.CornerRadius(UniformRadius(theme.chip_radius));
        section_chip.Padding(Thickness{ 12.0, 6.0, 12.0, 6.0 });
        section_chip.Background(make_brush(theme.chip_active_background));
        m_section_text = TextBlock{};
        m_section_text.Text(L"Overview");
        m_section_text.Foreground(make_brush(theme.text_primary));
        section_chip.Child(m_section_text);
        status_row.Children().Append(section_chip);

        shell.Children().Append(status_row);
        TraceStartup(L"BuildShell after status row");

        auto breadcrumb_group = StackPanel{};
        breadcrumb_group.Spacing(6);

        auto breadcrumb_row = StackPanel{};
        breadcrumb_row.Orientation(Orientation::Horizontal);
        breadcrumb_row.Spacing(8);

        m_overview_breadcrumb_button = Button{};
        m_overview_breadcrumb_button.Content(box_value(L"Overview"));
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_overview_breadcrumb_button,
            L"Overview breadcrumb");
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            m_overview_breadcrumb_button,
            L"Return to the overview section.");
        m_overview_breadcrumb_button.Click({ this, &MainWindow::OnBreadcrumbClicked });
        breadcrumb_row.Children().Append(m_overview_breadcrumb_button);

        auto breadcrumb_sep1 = TextBlock{};
        breadcrumb_sep1.Text(L">");
        breadcrumb_sep1.VerticalAlignment(VerticalAlignment::Center);
        breadcrumb_sep1.Opacity(0.6);
        breadcrumb_row.Children().Append(breadcrumb_sep1);

        m_scan_root_button = Button{};
        m_scan_root_button.Content(box_value(L"Scan root"));
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_scan_root_button,
            L"Scan root breadcrumb");
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            m_scan_root_button,
            L"Focus the scan root path box.");
        m_scan_root_button.Click({ this, &MainWindow::OnBreadcrumbClicked });
        breadcrumb_row.Children().Append(m_scan_root_button);

        auto breadcrumb_sep2 = TextBlock{};
        breadcrumb_sep2.Text(L">");
        breadcrumb_sep2.VerticalAlignment(VerticalAlignment::Center);
        breadcrumb_sep2.Opacity(0.6);
        breadcrumb_row.Children().Append(breadcrumb_sep2);

        m_root_breadcrumb_text = TextBlock{};
        m_root_breadcrumb_text.Text(L"C:\\");
        m_root_breadcrumb_text.VerticalAlignment(VerticalAlignment::Center);
        m_root_breadcrumb_text.Opacity(0.75);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_root_breadcrumb_text,
            L"Current scan root breadcrumb");
        breadcrumb_row.Children().Append(m_root_breadcrumb_text);

        breadcrumb_group.Children().Append(breadcrumb_row);

        m_path_breadcrumb_panel = StackPanel{};
        m_path_breadcrumb_panel.Orientation(Orientation::Horizontal);
        m_path_breadcrumb_panel.Spacing(6);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_path_breadcrumb_panel,
            L"Path breadcrumbs");
        breadcrumb_group.Children().Append(m_path_breadcrumb_panel);

        shell.Children().Append(breadcrumb_group);
        TraceStartup(L"BuildShell after breadcrumb row");

        {
            auto scan_card = Border{};
            apply_card_style(scan_card);

            auto scan_stack = StackPanel{};
            scan_stack.Spacing(10);
            scan_card.Child(scan_stack);

            scan_stack.Children().Append(make_card_title(L"Scan"));

            auto root_row = StackPanel{};
            root_row.Orientation(Orientation::Horizontal);
            root_row.Spacing(12);

            m_root_path_box = TextBox{};
            m_root_path_box.MinWidth(360);
            m_root_path_box.Text(m_current_root_path.c_str());
            m_root_path_box.PlaceholderText(L"Root path to scan");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_root_path_box,
                L"Scan root path");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_root_path_box,
                L"Enter the folder or volume root to scan.");
            root_row.Children().Append(m_root_path_box);

            m_start_scan_button = Button{};
            m_start_scan_button.Content(box_value(L"Start scan"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_start_scan_button,
                L"Start scan");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_start_scan_button,
                L"Start a full scan of the configured root path.");
            m_start_scan_button.Click({ this, &MainWindow::OnStartClicked });
            root_row.Children().Append(m_start_scan_button);

            m_incremental_scan_button = Button{};
            m_incremental_scan_button.Content(box_value(L"Incremental rescan"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_incremental_scan_button,
                L"Incremental rescan");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_incremental_scan_button,
                L"Rescan the configured root path using the existing catalog snapshot.");
            m_incremental_scan_button.Click({ this, &MainWindow::OnIncrementalScanClicked });
            root_row.Children().Append(m_incremental_scan_button);

            m_cancel_scan_button = Button{};
            m_cancel_scan_button.Content(box_value(L"Cancel"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_cancel_scan_button,
                L"Cancel");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_cancel_scan_button,
                L"Cancel the active scan. Escape also cancels.");
            m_cancel_scan_button.Click({ this, &MainWindow::OnCancelClicked });
            root_row.Children().Append(m_cancel_scan_button);

            scan_stack.Children().Append(root_row);

            auto progress_track = Border{};
            progress_track.Width(360.0);
            progress_track.Height(8.0);
            progress_track.HorizontalAlignment(HorizontalAlignment::Left);
            progress_track.Background(make_brush(theme.progress_track));
            progress_track.CornerRadius(UniformRadius(theme.progress_radius));

            m_scan_progress_fill = Border{};
            m_scan_progress_fill.Width(0.0);
            m_scan_progress_fill.Height(8.0);
            m_scan_progress_fill.HorizontalAlignment(HorizontalAlignment::Left);
            m_scan_progress_fill.Background(make_brush(theme.progress_fill));
            m_scan_progress_fill.CornerRadius(UniformRadius(theme.progress_radius));
            progress_track.Child(m_scan_progress_fill);
            scan_stack.Children().Append(progress_track);

            m_progress_text = TextBlock{};
            m_progress_text.Text(L"0% complete");
            m_progress_text.Opacity(0.75);
            m_progress_text.TextWrapping(TextWrapping::WrapWholeWords);
            scan_stack.Children().Append(m_progress_text);

            shell.Children().Append(scan_card);
        }
        TraceStartup(L"BuildShell after compact scan controls");

        {
            auto banner_stack = StackPanel{};
            banner_stack.Spacing(8);

            auto make_state_banner = [&](Border& storage, std::wstring_view title, std::wstring_view body, Windows::UI::Color const& background, Windows::UI::Color const& border) {
                auto banner = Border{};
                storage = banner;
                banner.CornerRadius(UniformRadius(theme.panel_radius));
                banner.Padding(Thickness{ 14.0, 12.0, 14.0, 12.0 });
                banner.Background(make_brush(background));
                banner.BorderBrush(make_brush(border));
                banner.BorderThickness(Thickness{ 1.0, 1.0, 1.0, 1.0 });

                auto stack = StackPanel{};
                stack.Spacing(4);
                banner.Child(stack);

                auto title_text = TextBlock{};
                title_text.Text(winrt::hstring(title));
                title_text.FontSize(16);
                title_text.Foreground(make_brush(theme.text_primary));
                stack.Children().Append(title_text);

                auto body_text = TextBlock{};
                body_text.Text(winrt::hstring(body));
                body_text.Opacity(0.75);
                body_text.TextWrapping(TextWrapping::WrapWholeWords);
                stack.Children().Append(body_text);

                banner_stack.Children().Append(banner);
            };

            make_state_banner(
                m_loading_banner,
                L"Loading shell",
                L"Preparing the native UI and scan boundary.",
                theme.card_background,
                theme.card_border);
            make_state_banner(
                m_scanning_banner,
                L"Scanning in progress",
                L"Partial results will appear as the scanner emits them.",
                theme.chip_active_background,
                theme.progress_fill);
            make_state_banner(
                m_empty_banner,
                L"No scan results yet",
                L"Start a scan to populate the tree, treemap, and detail cards.",
                theme.card_background,
                theme.card_border);
            make_state_banner(
                m_error_banner,
                L"Scan error",
                L"A recoverable scan issue occurred.",
                theme.error_background,
                theme.error_border);

            m_error_text = TextBlock{};
            m_error_text.Text(L"A recoverable scan issue occurred.");
            m_error_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_error_text.Visibility(Visibility::Collapsed);
            banner_stack.Children().Append(m_error_text);

            m_scanning_banner.Visibility(Visibility::Collapsed);
            m_error_banner.Visibility(Visibility::Collapsed);

            shell.Children().Append(banner_stack);
        }
        TraceStartup(L"BuildShell after state banners");

        auto nav_strip = Border{};
        apply_compact_card_style(nav_strip);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            nav_strip,
            L"Shell menus");
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            nav_strip,
            L"Compact shell menus. Use Navigate for sections and View for optional panels.");

        auto menu_row = StackPanel{};
        menu_row.Orientation(Orientation::Horizontal);
        menu_row.Spacing(10);
        nav_strip.Child(menu_row);

        m_nav_chips.clear();
        auto navigate_label = TextBlock{};
        navigate_label.Text(L"Navigate");
        navigate_label.VerticalAlignment(VerticalAlignment::Center);
        navigate_label.Opacity(0.76);
        menu_row.Children().Append(navigate_label);

        m_section_menu = ComboBox{};
        m_section_menu.Width(170.0);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(m_section_menu, L"Navigate menu");
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            m_section_menu,
            L"Choose the active section. Keyboard shortcuts Ctrl or Alt plus 1 through 5 still work.");
        auto append_section_item = [&](std::wstring_view text) {
            auto item = ComboBoxItem{};
            item.Content(box_value(winrt::hstring(text)));
            m_section_menu.Items().Append(item);
        };
        append_section_item(L"Overview");
        append_section_item(L"Folder tree");
        append_section_item(L"Treemap");
        append_section_item(L"Search");
        append_section_item(L"Diagnostics");
        m_section_menu.SelectedIndex(SectionMenuIndex(m_active_section));
        m_section_menu.SelectionChanged({ this, &MainWindow::OnSectionMenuSelectionChanged });
        menu_row.Children().Append(m_section_menu);

        auto view_label = TextBlock{};
        view_label.Text(L"View");
        view_label.VerticalAlignment(VerticalAlignment::Center);
        view_label.Opacity(0.76);
        menu_row.Children().Append(view_label);

        auto make_view_toggle = [&](CheckBox& storage, std::wstring_view text, std::wstring_view help_text) {
            auto item = CheckBox{};
            storage = item;
            item.Content(box_value(winrt::hstring(text)));
            item.IsChecked(false);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(item, winrt::hstring(text));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(item, winrt::hstring(help_text));
            item.Click({ this, &MainWindow::OnOptionalPanelToggleClicked });
            menu_row.Children().Append(item);
        };
        make_view_toggle(m_current_state_toggle, L"Current state", L"Show or hide the current state panel.");
        make_view_toggle(m_folder_view_toggle, L"Folder view", L"Show or hide the folder and file detail panel.");
        make_view_toggle(m_folder_tree_toggle, L"Folder tree", L"Show or hide the virtualized folder tree panel.");
        make_view_toggle(m_runtime_metrics_toggle, L"Runtime metrics", L"Show or hide runtime metrics at the bottom of the UI.");

        shell.Children().Append(nav_strip);
        TraceStartup(L"BuildShell after menu strip");

        auto summary_card = Border{};
        m_overview_card = summary_card;
        apply_card_style(summary_card);

        auto summary_stack = StackPanel{};
        summary_stack.Spacing(6);
        summary_card.Child(summary_stack);

        summary_stack.Children().Append(make_card_title(L"Current state"));

        m_event_text = TextBlock{};
        m_event_text.Text(L"Ready to scan.");
        m_event_text.TextWrapping(TextWrapping::WrapWholeWords);
        summary_stack.Children().Append(m_event_text);

        m_summary_text = TextBlock{};
        m_summary_text.Text(L"Root path: C:\\ | Active section: Overview");
        m_summary_text.Opacity(0.75);
        m_summary_text.TextWrapping(TextWrapping::WrapWholeWords);
        summary_stack.Children().Append(m_summary_text);

        shell.Children().Append(summary_card);
        TraceStartup(L"BuildShell after summary card");

        {
            auto search_card = Border{};
            m_search_card = search_card;
            apply_card_style(search_card);

            auto search_stack = StackPanel{};
            search_stack.Spacing(8);
            search_card.Child(search_stack);

            search_stack.Children().Append(make_card_title(L"Search and filters"));

            auto search_row = StackPanel{};
            search_row.Orientation(Orientation::Horizontal);
            search_row.Spacing(12);

            m_search_box = TextBox{};
            m_search_box.Width(320.0);
            m_search_box.PlaceholderText(L"Search indexed files and folders");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_search_box,
                L"Search query");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_search_box,
                L"Search indexed file and folder names.");
            m_search_box.TextChanged({ this, &MainWindow::OnSearchQueryChanged });
            search_row.Children().Append(m_search_box);

            m_extension_box = TextBox{};
            m_extension_box.Width(180.0);
            m_extension_box.PlaceholderText(L"Extension filter");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_extension_box,
                L"Extension filter");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_extension_box,
                L"Limit search results to one or more file extensions.");
            m_extension_box.TextChanged({ this, &MainWindow::OnSearchQueryChanged });
            search_row.Children().Append(m_extension_box);

            m_min_size_box = TextBox{};
            m_min_size_box.Width(120.0);
            m_min_size_box.PlaceholderText(L"Min size, e.g. 10 MB");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_min_size_box,
                L"Minimum size filter");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_min_size_box,
                L"Limit search results to entries at or above a size such as 10 MB.");
            m_min_size_box.TextChanged({ this, &MainWindow::OnSearchQueryChanged });
            search_row.Children().Append(m_min_size_box);

            m_search_apply_button = Button{};
            m_search_apply_button.Content(box_value(L"Apply"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_search_apply_button,
                L"Apply search filters");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_search_apply_button,
                L"Apply the current search query and filters.");
            m_search_apply_button.Click({ this, &MainWindow::OnSearchClicked });
            search_row.Children().Append(m_search_apply_button);

            search_stack.Children().Append(search_row);
            TraceStartup(L"BuildShell search query row end");

            auto search_options_row = StackPanel{};
            search_options_row.Orientation(Orientation::Horizontal);
            search_options_row.Spacing(12);

            m_match_mode_box = ComboBox{};
            m_match_mode_box.Width(160.0);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_match_mode_box,
                L"Search match mode");
            auto substring_item = ComboBoxItem{};
            substring_item.Content(box_value(L"Substring"));
            m_match_mode_box.Items().Append(substring_item);
            auto exact_item = ComboBoxItem{};
            exact_item.Content(box_value(L"Exact"));
            m_match_mode_box.Items().Append(exact_item);
            m_match_mode_box.SelectedIndex(0);
            m_match_mode_box.SelectionChanged({ this, &MainWindow::OnSearchOptionsChanged });
            search_options_row.Children().Append(m_match_mode_box);

            m_sort_field_box = ComboBox{};
            m_sort_field_box.Width(150.0);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_sort_field_box,
                L"Search sort field");
            auto sort_name_item = ComboBoxItem{};
            sort_name_item.Content(box_value(L"Name"));
            m_sort_field_box.Items().Append(sort_name_item);
            auto sort_size_item = ComboBoxItem{};
            sort_size_item.Content(box_value(L"Size"));
            m_sort_field_box.Items().Append(sort_size_item);
            auto sort_type_item = ComboBoxItem{};
            sort_type_item.Content(box_value(L"Type"));
            m_sort_field_box.Items().Append(sort_type_item);
            m_sort_field_box.SelectedIndex(0);
            m_sort_field_box.SelectionChanged({ this, &MainWindow::OnSearchOptionsChanged });
            search_options_row.Children().Append(m_sort_field_box);

            m_sort_direction_box = ComboBox{};
            m_sort_direction_box.Width(170.0);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_sort_direction_box,
                L"Search sort direction");
            auto descending_item = ComboBoxItem{};
            descending_item.Content(box_value(L"Descending"));
            m_sort_direction_box.Items().Append(descending_item);
            auto ascending_item = ComboBoxItem{};
            ascending_item.Content(box_value(L"Ascending"));
            m_sort_direction_box.Items().Append(ascending_item);
            m_sort_direction_box.SelectedIndex(0);
            m_sort_direction_box.SelectionChanged({ this, &MainWindow::OnSearchOptionsChanged });
            search_options_row.Children().Append(m_sort_direction_box);

            search_stack.Children().Append(search_options_row);
            TraceStartup(L"BuildShell search options row end");

            auto search_date_row = StackPanel{};
            search_date_row.Orientation(Orientation::Horizontal);
            search_date_row.Spacing(12);

            m_modified_after_box = TextBox{};
            m_modified_after_box.Width(240.0);
            m_modified_after_box.PlaceholderText(L"Modified after UTC YYYY-MM-DD");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_modified_after_box,
                L"Modified after filter");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_modified_after_box,
                L"Limit search results to entries modified on or after a UTC date.");
            m_modified_after_box.TextChanged({ this, &MainWindow::OnSearchQueryChanged });
            search_date_row.Children().Append(m_modified_after_box);

            m_modified_before_box = TextBox{};
            m_modified_before_box.Width(240.0);
            m_modified_before_box.PlaceholderText(L"Modified before UTC YYYY-MM-DD");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_modified_before_box,
                L"Modified before filter");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_modified_before_box,
                L"Limit search results to entries modified before a UTC date.");
            m_modified_before_box.TextChanged({ this, &MainWindow::OnSearchQueryChanged });
            search_date_row.Children().Append(m_modified_before_box);

            search_stack.Children().Append(search_date_row);
            TraceStartup(L"BuildShell search date row end");

            m_search_preview_text = TextBlock{};
            m_search_preview_text.Text(L"Search indexed files and folders after a scan or cached catalog load.");
            m_search_preview_text.Opacity(0.75);
            m_search_preview_text.TextWrapping(TextWrapping::WrapWholeWords);
            search_stack.Children().Append(m_search_preview_text);

            m_search_results_panel = StackPanel{};
            m_search_results_panel.Spacing(6);
            search_stack.Children().Append(m_search_results_panel);

            shell.Children().Append(search_card);
        }
        TraceStartup(L"BuildShell search card end");

        {
            auto tree_card = Border{};
            m_tree_card = tree_card;
            apply_card_style(tree_card);

            auto tree_stack = StackPanel{};
            tree_stack.Spacing(8);
            tree_card.Child(tree_stack);

            tree_stack.Children().Append(make_card_title(L"Folder tree"));

            auto tree_subtitle = TextBlock{};
            tree_subtitle.Text(L"Catalog-backed tree rows and capped live list.");
            tree_subtitle.Opacity(0.75);
            tree_subtitle.TextWrapping(TextWrapping::WrapWholeWords);
            tree_stack.Children().Append(tree_subtitle);

            auto tree_action_row = StackPanel{};
            tree_action_row.Orientation(Orientation::Horizontal);
            tree_action_row.Spacing(10);

            m_tree_snapshot_expand_button = Button{};
            m_tree_snapshot_expand_button.Content(box_value(L"Load more rows"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_tree_snapshot_expand_button,
                L"Load more tree rows");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_tree_snapshot_expand_button,
                L"Show or hide additional catalog tree rows.");
            m_tree_snapshot_expand_button.Click({ this, &MainWindow::OnTreeSnapshotExpandClicked });
            tree_action_row.Children().Append(m_tree_snapshot_expand_button);

            m_tree_window_previous_button = Button{};
            m_tree_window_previous_button.Content(box_value(L"Previous rows"));
            m_tree_window_previous_button.IsEnabled(false);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_tree_window_previous_button,
                L"Previous tree rows");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_tree_window_previous_button,
                L"Move the virtualized tree list to the previous row window.");
            m_tree_window_previous_button.Click({ this, &MainWindow::OnTreeWindowPreviousClicked });
            tree_action_row.Children().Append(m_tree_window_previous_button);

            m_tree_window_next_button = Button{};
            m_tree_window_next_button.Content(box_value(L"Next rows"));
            m_tree_window_next_button.IsEnabled(false);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_tree_window_next_button,
                L"Next tree rows");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_tree_window_next_button,
                L"Move the virtualized tree list to the next row window.");
            m_tree_window_next_button.Click({ this, &MainWindow::OnTreeWindowNextClicked });
            tree_action_row.Children().Append(m_tree_window_next_button);

            auto tree_action_hint = TextBlock{};
            tree_action_hint.Text(L"Rows are paged in a virtualized window so large catalogs stay responsive.");
            tree_action_hint.Opacity(0.72);
            tree_action_hint.VerticalAlignment(VerticalAlignment::Center);
            tree_action_row.Children().Append(tree_action_hint);
            tree_stack.Children().Append(tree_action_row);

            m_tree_catalog.clear();
            m_tree_catalog_keys.clear();

            m_tree_snapshot_panel = StackPanel{};
            m_tree_snapshot_panel.Spacing(6);
            tree_stack.Children().Append(m_tree_snapshot_panel);

            m_tree_snapshot_extra_panel = StackPanel{};
            m_tree_snapshot_extra_panel.Spacing(6);
            m_tree_snapshot_extra_panel.Visibility(Visibility::Collapsed);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_tree_snapshot_extra_panel,
                L"Additional tree rows");
            tree_stack.Children().Append(m_tree_snapshot_extra_panel);

            m_tree_list_status_text = TextBlock{};
            m_tree_list_status_text.Text(L"Live list is waiting for catalog rows.");
            m_tree_list_status_text.Opacity(0.72);
            m_tree_list_status_text.TextWrapping(TextWrapping::WrapWholeWords);
            tree_stack.Children().Append(m_tree_list_status_text);

            auto tree_list_header = StackPanel{};
            tree_list_header.Orientation(Orientation::Horizontal);
            tree_list_header.Spacing(12);
            tree_list_header.Margin(Thickness{ 12.0, 4.0, 0.0, 0.0 });
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                tree_list_header,
                L"Tree list columns");

            auto make_tree_header_label = [&](std::wstring_view text, double width) {
                auto label = TextBlock{};
                label.Text(winrt::hstring(text));
                label.Width(width);
                label.Opacity(0.68);
                label.Foreground(make_brush(theme.text_primary));
                return label;
            };
            tree_list_header.Children().Append(make_tree_header_label(L"Name", 226.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Usage", 220.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Size", 72.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Kind", 82.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Level", 64.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Parent", 180.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Path", 360.0));
            tree_stack.Children().Append(tree_list_header);

            m_tree_list_view = ListView{};
            m_tree_list_view.MaxHeight(360.0);
            m_tree_list_view.ItemsPanel(
                Microsoft::UI::Xaml::Markup::XamlReader::Load(
                    LR"(<ItemsPanelTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><ItemsStackPanel Orientation="Vertical"/></ItemsPanelTemplate>)")
                    .as<Microsoft::UI::Xaml::Controls::ItemsPanelTemplate>());
            m_tree_list_view.SelectionMode(ListViewSelectionMode::Single);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_tree_list_view,
                L"Virtualized catalog tree rows");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_tree_list_view,
                L"Paged virtualized list of catalog entries. Select a row to update details and treemap focus.");
            m_tree_list_view.SelectionChanged({ this, &MainWindow::OnTreeSelectionChanged });
            tree_stack.Children().Append(m_tree_list_view);
            m_tree_updates_ready = true;

            shell.Children().Append(tree_card);
            UpdateTreeSnapshotPreview(m_tree_catalog);
        }
        TraceStartup(L"BuildShell tree card end");

        {
            auto catalog_card = Border{};
            apply_card_style(catalog_card);

            auto catalog_stack = StackPanel{};
            catalog_stack.Spacing(6);
            catalog_card.Child(catalog_stack);

            catalog_stack.Children().Append(make_card_title(L"Catalog preview"));

            m_catalog_snapshot_text = TextBlock{};
            m_catalog_snapshot_text.Text(L"Top entries will appear here.");
            m_catalog_snapshot_text.TextWrapping(TextWrapping::WrapWholeWords);
            catalog_stack.Children().Append(m_catalog_snapshot_text);

            shell.Children().Append(catalog_card);
        }
        TraceStartup(L"BuildShell catalog card end");

        {
            auto detail_card = Border{};
            m_detail_card = detail_card;
            apply_card_style(detail_card);

            auto detail_stack = StackPanel{};
            detail_stack.Spacing(6);
            detail_card.Child(detail_stack);

            detail_stack.Children().Append(make_card_title(L"Details"));

            m_selection_text = TextBlock{};
            m_selection_text.Text(L"Selection: Root volume (C:\\)");
            m_selection_text.TextWrapping(TextWrapping::WrapWholeWords);
            detail_stack.Children().Append(m_selection_text);

            m_selection_size_text = TextBlock{};
            m_selection_size_text.Text(L"Size: 0 B");
            m_selection_size_text.TextWrapping(TextWrapping::WrapWholeWords);
            detail_stack.Children().Append(m_selection_size_text);

            auto make_detail_panel = [&](Border& storage, std::wstring_view title, std::wstring_view subtitle, Windows::UI::Color const& background, Windows::UI::Color const& border) {
                auto panel_border = Border{};
                storage = panel_border;
                ApplyAccentPanelStyle(panel_border, background, border);

                auto panel = StackPanel{};
                panel.Spacing(4);
                panel_border.Child(panel);

                auto title_text = TextBlock{};
                title_text.Text(winrt::hstring(title));
                title_text.Foreground(make_brush(theme.text_on_accent));
                panel.Children().Append(title_text);

                auto subtitle_text = TextBlock{};
                subtitle_text.Text(winrt::hstring(subtitle));
                subtitle_text.Foreground(make_brush(theme.text_on_accent));
                subtitle_text.Opacity(0.82);
                subtitle_text.TextWrapping(TextWrapping::WrapWholeWords);
                panel.Children().Append(subtitle_text);

                detail_stack.Children().Append(panel_border);
            };

            make_detail_panel(
                m_volume_detail_panel,
                L"Volume details",
                L"Mount point, root directory, and scan status for the selected volume.",
                theme.volume_accent,
                theme.subtle_border);
            make_detail_panel(
                m_folder_detail_panel,
                L"Folder details",
                L"Directory totals, child count, and aggregate usage for the selected folder.",
                theme.folder_accent,
                theme.subtle_border);
            make_detail_panel(
                m_file_detail_panel,
                L"File details",
                L"Size, allocation size, timestamps, and metadata for the selected file.",
                theme.file_accent,
                theme.subtle_border);

            shell.Children().Append(detail_card);
        }
        TraceStartup(L"BuildShell detail card end");

        {
            auto treemap_card = Border{};
            m_treemap_card = treemap_card;
            apply_card_style(treemap_card);

            auto treemap_stack = StackPanel{};
            treemap_stack.Spacing(8);
            treemap_card.Child(treemap_stack);

            treemap_stack.Children().Append(make_card_title(L"Treemap"));

            auto treemap_subtitle = TextBlock{};
            treemap_subtitle.Text(L"Scan or load a cached catalog to render proportional usage tiles.");
            treemap_subtitle.Opacity(0.75);
            treemap_subtitle.TextWrapping(TextWrapping::WrapWholeWords);
            treemap_stack.Children().Append(treemap_subtitle);

            m_treemap_surface = SwapChainPanel{};
            m_treemap_surface.Height(120.0);
            m_treemap_surface.MinHeight(96.0);
            m_treemap_surface.HorizontalAlignment(HorizontalAlignment::Stretch);
            m_treemap_surface.SizeChanged({ this, &MainWindow::OnTreemapSurfaceSizeChanged });
            m_treemap_surface.PointerMoved({ this, &MainWindow::OnTreemapSurfacePointerMoved });
            m_treemap_surface.PointerExited({ this, &MainWindow::OnTreemapSurfacePointerExited });
            m_treemap_surface.Tapped({ this, &MainWindow::OnTreemapSurfaceTapped });
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_treemap_surface,
                L"GPU treemap surface");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_treemap_surface,
                L"Direct2D SwapChainPanel surface showing catalog usage tiles. Hover or tap a tile to update selection details.");
            treemap_stack.Children().Append(m_treemap_surface);

            m_treemap_render_status = ProbeTreemapRenderStack();
            m_treemap_surface_status_text = TextBlock{};
            m_treemap_surface_status_text.Text(winrt::hstring(
                L"SwapChainPanel host is initialized. " + m_treemap_render_status));
            m_treemap_surface_status_text.Opacity(0.72);
            m_treemap_surface_status_text.TextWrapping(TextWrapping::WrapWholeWords);
            treemap_stack.Children().Append(m_treemap_surface_status_text);

            m_treemap_zoom_overlay = Border{};
            m_treemap_zoom_overlay.CornerRadius(UniformRadius(theme.panel_radius));
            m_treemap_zoom_overlay.Padding(Thickness{ 18.0, 18.0, 18.0, 18.0 });
            m_treemap_zoom_overlay.Background(make_brush(Windows::UI::Colors::Transparent()));
            m_treemap_zoom_overlay.BorderBrush(make_brush(theme.chip_active_background));
            m_treemap_zoom_overlay.BorderThickness(Thickness{ 1.0, 1.0, 1.0, 1.0 });
            m_treemap_zoom_overlay.Visibility(Visibility::Collapsed);
            m_treemap_zoom_overlay.IsHitTestVisible(false);
            auto zoom_panel = StackPanel{};
            zoom_panel.VerticalAlignment(VerticalAlignment::Center);
            zoom_panel.HorizontalAlignment(HorizontalAlignment::Center);
            zoom_panel.Spacing(6);
            m_treemap_zoom_overlay.Child(zoom_panel);
            m_treemap_zoom_title = TextBlock{};
            m_treemap_zoom_title.Text(L"Catalog tile");
            zoom_panel.Children().Append(m_treemap_zoom_title);
            m_treemap_zoom_description = TextBlock{};
            m_treemap_zoom_description.Text(L"Hover or tap a rendered catalog tile to inspect it.");
            m_treemap_zoom_description.Opacity(0.8);
            m_treemap_zoom_description.TextWrapping(TextWrapping::WrapWholeWords);
            m_treemap_zoom_description.TextAlignment(TextAlignment::Center);
            zoom_panel.Children().Append(m_treemap_zoom_description);
            treemap_stack.Children().Append(m_treemap_zoom_overlay);

            shell.Children().Append(treemap_card);
        }
        TraceStartup(L"BuildShell treemap card end");

        {
            auto runtime_card = Border{};
            m_diagnostics_card = runtime_card;
            apply_card_style(runtime_card);

            auto runtime_stack = StackPanel{};
            runtime_stack.Spacing(6);
            runtime_card.Child(runtime_stack);

            runtime_stack.Children().Append(make_card_title(L"Runtime metrics"));

            m_developer_diagnostics_toggle = CheckBox{};
            m_developer_diagnostics_toggle.Content(box_value(L"Developer diagnostics"));
            m_developer_diagnostics_toggle.IsChecked(true);
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                m_developer_diagnostics_toggle,
                L"Developer diagnostics");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                m_developer_diagnostics_toggle,
                L"Show or hide correctness, recent issue, and runtime diagnostic details.");
            m_developer_diagnostics_toggle.Checked({ this, &MainWindow::OnDeveloperDiagnosticsToggled });
            m_developer_diagnostics_toggle.Unchecked({ this, &MainWindow::OnDeveloperDiagnosticsToggled });
            runtime_stack.Children().Append(m_developer_diagnostics_toggle);

            m_developer_diagnostics_panel = StackPanel{};
            m_developer_diagnostics_panel.Spacing(6);
            runtime_stack.Children().Append(m_developer_diagnostics_panel);

            m_runtime_snapshot_text = TextBlock{};
            m_runtime_snapshot_text.Text(L"UI batching: startup, flushes=0, queued events=0, last latency=0 ms, last input=0 ms, flush cost=0 ms");
            m_runtime_snapshot_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_runtime_snapshot_text);

            m_performance_text = TextBlock{};
            m_performance_text.Text(L"Performance counters are ready.");
            m_performance_text.Opacity(0.75);
            m_performance_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_performance_text);

            m_correctness_text = TextBlock{};
            m_correctness_text.Text(L"Correctness: waiting for scan summary.");
            m_correctness_text.Opacity(0.75);
            m_correctness_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_correctness_text);

            m_recent_issues_text = TextBlock{};
            m_recent_issues_text.Text(L"Recent issues: none");
            m_recent_issues_text.Opacity(0.75);
            m_recent_issues_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_recent_issues_text);

            m_issue_drilldown_text = TextBlock{};
            m_issue_drilldown_text.Text(L"Issue drill-down: errors=0, skipped=0, transient=0, permissions=0, missing=0, last=none");
            m_issue_drilldown_text.Opacity(0.75);
            m_issue_drilldown_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_issue_drilldown_text);

            shell.Children().Append(runtime_card);
        }
        TraceStartup(L"BuildShell runtime card end");

        {
            auto shell_scroll = ScrollViewer{};
            shell_scroll.VerticalScrollBarVisibility(ScrollBarVisibility::Auto);
            shell_scroll.HorizontalScrollBarVisibility(ScrollBarVisibility::Disabled);
            shell_scroll.Content(shell);

            root.Children().Append(shell_scroll);
            Content(root);
        }
        TraceStartup(L"BuildShell stable shell end");
        return;
    }

    void MainWindow::OnWindowLoaded(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        TraceStartup(L"OnWindowLoaded begin");
        if (!m_composition_rendering_registered) {
            m_composition_rendering_token = Microsoft::UI::Xaml::Media::CompositionTarget::Rendering(
                { this, &MainWindow::OnCompositionRendering });
            m_composition_rendering_registered = true;
            TraceStartup(L"OnWindowLoaded composition rendering registered");
        }
        UpdateBreadcrumbs();
        TraceStartup(L"OnWindowLoaded after breadcrumbs");
        UpdateSearchPreview(FormatSearchQuery());
        TraceStartup(L"OnWindowLoaded after search preview");
        LoadPersistedCatalogSnapshot();
        TraceStartup(L"OnWindowLoaded after persisted catalog snapshot");
    }

    void MainWindow::OnWindowClosed(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::WindowEventArgs const&)
    {
        if (m_composition_rendering_registered) {
            Microsoft::UI::Xaml::Media::CompositionTarget::Rendering(m_composition_rendering_token);
            m_composition_rendering_registered = false;
        }
        if (m_treemap_render_timer) {
            m_treemap_render_timer.Stop();
        }
        if (m_ui_flush_timer) {
            m_ui_flush_timer.Stop();
        }
    }

    void MainWindow::OnCompositionRendering(
        winrt::Windows::Foundation::IInspectable const&,
        winrt::Windows::Foundation::IInspectable const&)
    {
        const auto now = std::chrono::steady_clock::now();
        if (m_last_composition_frame_time.time_since_epoch().count() > 0) {
            m_last_composition_frame_ms = std::chrono::duration<double, std::milli>(
                now - m_last_composition_frame_time).count();
            m_peak_composition_frame_ms = (std::max)(
                m_peak_composition_frame_ms,
                m_last_composition_frame_ms);
        }
        m_last_composition_frame_time = now;
        ++m_total_composition_frame_count;
    }

    void MainWindow::OnWindowKeyDown(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& args)
    {
        const auto input_started = std::chrono::steady_clock::now();
        const bool ctrl_pressed = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        const bool alt_pressed = (GetKeyState(VK_MENU) & 0x8000) != 0;
        const bool navigation_modifier_pressed = ctrl_pressed || alt_pressed;
        const auto key = args.Key();
        bool handled = false;

        if (ctrl_pressed && key == winrt::Windows::System::VirtualKey::F) {
            FocusSearchBox();
            NavigateToSection(ShellSection::Search);
            handled = true;
        }
        else if (ctrl_pressed && key == winrt::Windows::System::VirtualKey::L) {
            FocusRootPathBox();
            handled = true;
        }
        else if (ctrl_pressed && key == winrt::Windows::System::VirtualKey::S) {
            BeginScanFromCurrentRoot();
            handled = true;
        }
        else if (navigation_modifier_pressed && key == winrt::Windows::System::VirtualKey::Number1) {
            NavigateToSection(ShellSection::Overview);
            handled = true;
        }
        else if (navigation_modifier_pressed && key == winrt::Windows::System::VirtualKey::Number2) {
            NavigateToSection(ShellSection::Tree);
            handled = true;
        }
        else if (navigation_modifier_pressed && key == winrt::Windows::System::VirtualKey::Number3) {
            NavigateToSection(ShellSection::Treemap);
            handled = true;
        }
        else if (navigation_modifier_pressed && key == winrt::Windows::System::VirtualKey::Number4) {
            NavigateToSection(ShellSection::Search);
            handled = true;
        }
        else if (navigation_modifier_pressed && key == winrt::Windows::System::VirtualKey::Number5) {
            NavigateToSection(ShellSection::Diagnostics);
            handled = true;
        }
        else if (key == winrt::Windows::System::VirtualKey::Escape && m_session_active) {
            auto session = m_session;
            m_session_active = false;
            m_session = {};
            UpdateStatus(L"Cancelled.");
            ApplyShellState();
            std::thread([session]() {
                ::WinBlaze::UI::NativeBridge::CancelScan(session);
                ::WinBlaze::UI::NativeBridge::DestroyScan(session);
            }).detach();
            handled = true;
        }

        if (handled) {
            args.Handled(true);
            m_last_input_latency_ms = std::chrono::duration<double, std::milli>(
                std::chrono::steady_clock::now() - input_started).count();
            UpdatePerformanceCounters(L"input handled");
        }
    }

    void MainWindow::OnStartClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        BeginScanFromCurrentRoot();
    }

    void MainWindow::OnIncrementalScanClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        BeginScanFromCurrentRoot(true);
    }

    void MainWindow::BeginScanFromCurrentRoot(bool incremental)
    {
        if (m_session_active) {
            UpdateStatus(L"Scan already running.");
            return;
        }
        const std::wstring root_path = RootPathBox() ? RootPathBox().Text().c_str() : m_current_root_path;
        TraceStartup(std::wstring(incremental ? L"BeginIncrementalScan root=" : L"BeginScanFromCurrentRoot root=") + root_path);
        m_current_root_path = root_path;
        m_has_results = false;
        m_has_error = false;
        m_scan_started_at = std::chrono::steady_clock::now();
        m_last_scan_duration_text = L"Scan duration: in progress";
        UpdateProgress(0.0, L"0% complete");
        if (!m_ui_flush_timer) {
            m_ui_flush_timer = Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread().CreateTimer();
            m_ui_flush_timer.Interval(std::chrono::milliseconds(16));
            m_ui_flush_timer.Tick([this](auto const&, auto const&) {
                FlushPendingUiState();
            });
            TraceStartup(L"OnStartClicked ui flush timer created");
        }
        ::WinBlaze::UI::NativeBridge::Initialize();
        TraceStartup(L"OnStartClicked native bridge initialized");
        ApplyShellState();
        NavigateToSection(ShellSection::Overview);
        UpdateBreadcrumbs();

        auto handler = [this](WbEvent const& event) {
            HandleNativeEvent(event);
        };
        m_session = incremental
            ? ::WinBlaze::UI::NativeBridge::StartIncrementalScan(root_path.c_str(), handler)
            : ::WinBlaze::UI::NativeBridge::StartScan(root_path.c_str(), handler);
        m_session_active = true;
        UpdateStatus(incremental ? L"Incremental rescan..." : L"Scanning...");
        UpdateEventText(incremental ? L"Incremental rescan started." : L"Scan started.");
        ApplyShellState();
    }

    void MainWindow::OnCancelClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (!m_session_active) {
            UpdateStatus(L"No active scan.");
            return;
        }

        auto session = m_session;
        m_session_active = false;
        m_session = {};
        UpdateStatus(L"Cancelled.");
        ApplyShellState();
        std::thread([session]() {
            ::WinBlaze::UI::NativeBridge::CancelScan(session);
            ::WinBlaze::UI::NativeBridge::DestroyScan(session);
        }).detach();
    }

    void MainWindow::OnStartTapped(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&)
    {
        BeginScanFromCurrentRoot();
    }

    void MainWindow::OnSearchClicked(winrt::Windows::Foundation::IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        m_tree_window_offset = 0;
        RefreshInstantSearch();
        UpdateStatus(L"Search results updated.");
        NavigateToSection(ShellSection::Search);
    }

    void MainWindow::OnSearchQueryChanged(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Controls::TextChangedEventArgs const&)
    {
        m_tree_window_offset = 0;
        RefreshInstantSearch();
    }

    void MainWindow::OnSearchOptionsChanged(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&)
    {
        m_tree_window_offset = 0;
        RefreshInstantSearch();
    }

    void MainWindow::OnDeveloperDiagnosticsToggled(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (!DeveloperDiagnosticsToggle() || !DeveloperDiagnosticsPanel()) {
            return;
        }

        const auto is_checked = DeveloperDiagnosticsToggle().IsChecked();
        const bool visible = is_checked && is_checked.Value();
        SetControlVisibility(DeveloperDiagnosticsPanel(), visible);
        UpdateStatus(visible ? L"Developer diagnostics shown." : L"Developer diagnostics hidden.");
    }

    void MainWindow::OnOptionalPanelToggleClicked(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        auto is_checked = [](Microsoft::UI::Xaml::Controls::CheckBox item) {
            if (!item) {
                return false;
            }
            const auto value = item.IsChecked();
            return value && value.Value();
        };

        m_show_current_state = is_checked(CurrentStateToggle());
        m_show_folder_view = is_checked(FolderViewToggle());
        m_show_folder_tree = is_checked(FolderTreeToggle());
        m_show_runtime_metrics = is_checked(RuntimeMetricsToggle());

        SetSection(m_active_section);
        UpdateStatus(L"View options updated.");
    }

    void MainWindow::OnSectionMenuSelectionChanged(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&)
    {
        if (m_section_menu_updates_suppressed || !SectionMenu()) {
            return;
        }

        NavigateToSection(SectionFromMenuIndex(SectionMenu().SelectedIndex()));
    }

    void MainWindow::OnBreadcrumbClicked(winrt::Windows::Foundation::IInspectable const& sender, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (auto button = sender.try_as<Microsoft::UI::Xaml::Controls::Button>()) {
            const std::wstring text = winrt::unbox_value_or<winrt::hstring>(button.Content(), winrt::hstring{}).c_str();
            const std::wstring tag = winrt::unbox_value_or<winrt::hstring>(button.Tag(), winrt::hstring{}).c_str();
            if (!tag.empty()) {
                m_current_root_path = tag;
                if (RootPathBox()) {
                    RootPathBox().Text(winrt::hstring(tag));
                }
                UpdateBreadcrumbs();
                UpdateStatus(L"Breadcrumb path updated.");
                UpdateEventText(L"Scan root set to " + tag);
                return;
            }
            if (text == L"Overview") {
                NavigateToSection(ShellSection::Overview);
            } else {
                FocusRootPathBox();
            }
        }
    }

    void MainWindow::OnTreeItemClicked(winrt::Windows::Foundation::IInspectable const& sender, Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (auto button = sender.try_as<Microsoft::UI::Xaml::Controls::Button>()) {
            const std::wstring name = FirstTextBlockText(button.Content());
            const std::wstring tag = winrt::unbox_value_or<winrt::hstring>(button.Tag(), L"").c_str();
            const auto first = tag.find(L'|');
            const auto second = tag.find(L'|', first == std::wstring::npos ? first : first + 1);
            const auto third = tag.find(L'|', second == std::wstring::npos ? second : second + 1);
            if (first != std::wstring::npos && second != std::wstring::npos && third != std::wstring::npos) {
                SelectVisualizationTarget(
                    name,
                    tag.substr(0, first),
                    tag.substr(first + 1, second - first - 1),
                    tag.substr(second + 1, third - second - 1));
            }
        }
    }

    void MainWindow::OnSearchResultClicked(
        winrt::Windows::Foundation::IInspectable const& sender,
        Microsoft::UI::Xaml::RoutedEventArgs const& args)
    {
        OnTreeItemClicked(sender, args);
        NavigateToSection(ShellSection::Tree);
        UpdateStatus(L"Search result selected.");
        UpdateEventText(L"Opened selected search result in Tree.");
    }

    void MainWindow::OnTreeSnapshotExpandClicked(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (!TreeSnapshotExtraPanel()) {
            return;
        }

        const auto next = TreeSnapshotExtraPanel().Visibility() == Microsoft::UI::Xaml::Visibility::Visible
            ? Microsoft::UI::Xaml::Visibility::Collapsed
            : Microsoft::UI::Xaml::Visibility::Visible;
        TreeSnapshotExtraPanel().Visibility(next);

        if (TreeSnapshotExpandButton()) {
            TreeSnapshotExpandButton().Content(box_value(
                next == Microsoft::UI::Xaml::Visibility::Visible ? L"Hide extra rows" : L"Load more rows"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                TreeSnapshotExpandButton(),
                next == Microsoft::UI::Xaml::Visibility::Visible ? L"Hide extra tree rows" : L"Load more tree rows");
        }

        UpdateStatus(next == Microsoft::UI::Xaml::Visibility::Visible ? L"Tree rows expanded." : L"Tree rows collapsed.");
        UpdateEventText(next == Microsoft::UI::Xaml::Visibility::Visible ? L"Showing expanded tree rows." : L"Showing base tree rows.");
    }

    void MainWindow::OnTreeWindowPreviousClicked(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (m_tree_window_offset >= kTreeListVirtualizedWindowLimit) {
            m_tree_window_offset -= kTreeListVirtualizedWindowLimit;
        } else {
            m_tree_window_offset = 0;
        }

        UpdateTreeSnapshotPreview(FilterTreeCatalog());
        UpdateStatus(L"Tree list moved to the previous row window.");
    }

    void MainWindow::OnTreeWindowNextClicked(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        const auto entries = FilterTreeCatalog();
        if (m_tree_window_offset + kTreeListVirtualizedWindowLimit < entries.size()) {
            m_tree_window_offset += kTreeListVirtualizedWindowLimit;
        }

        UpdateTreeSnapshotPreview(entries);
        UpdateStatus(L"Tree list moved to the next row window.");
    }

    void MainWindow::OnTreeSelectionChanged(
        winrt::Windows::Foundation::IInspectable const& sender,
        Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&)
    {
        if (!m_tree_updates_ready || m_tree_selection_updates_suppressed) {
            return;
        }
        if (auto list_view = sender.try_as<Microsoft::UI::Xaml::Controls::ListView>()) {
            if (auto item = list_view.SelectedItem().try_as<Microsoft::UI::Xaml::Controls::ListViewItem>()) {
            std::wstring name = FirstTextBlockText(item.Content());
            if (name.empty()) {
                name = winrt::unbox_value_or<winrt::hstring>(item.Content(), winrt::hstring{}).c_str();
            }
            const std::wstring tag = winrt::unbox_value_or<winrt::hstring>(item.Tag(), L"").c_str();
            const auto first = tag.find(L'|');
            const auto second = tag.find(L'|', first == std::wstring::npos ? first : first + 1);
            const auto third = tag.find(L'|', second == std::wstring::npos ? second : second + 1);
            if (first != std::wstring::npos && second != std::wstring::npos && third != std::wstring::npos) {
                SelectVisualizationTarget(
                    name,
                    tag.substr(0, first),
                    tag.substr(first + 1, second - first - 1),
                    tag.substr(second + 1, third - second - 1));
            }
            }
        }
    }

    void MainWindow::OnTreemapSurfaceSizeChanged(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::SizeChangedEventArgs const& args)
    {
        if (!TreemapSurfaceStatusText()) {
            return;
        }

        const auto size = args.NewSize();
        m_treemap_render_dirty = true;
        ScheduleTreemapRender(L"treemap surface resized");
        TreemapSurfaceStatusText().Text(winrt::hstring(
            L"SwapChainPanel host active: " +
            std::to_wstring(static_cast<int>(size.Width)) + L"x" +
            std::to_wstring(static_cast<int>(size.Height)) +
            L" px. " + m_treemap_render_status));
        UpdatePerformanceCounters(L"treemap surface resized");
    }

    void MainWindow::RenderTreemapProbeFrame(int width, int height)
    {
        if (!TreemapSurface()) {
            return;
        }
        width = (std::max)(1, width);
        height = (std::max)(1, height);
        if (m_treemap_probe_frame_rendered &&
            !m_treemap_render_dirty &&
            width == m_treemap_render_width &&
            height == m_treemap_render_height) {
            return;
        }

        try {
            winrt::com_ptr<ID3D11Device> d3d_device;
            winrt::com_ptr<ID3D11DeviceContext> d3d_context;
            D3D_FEATURE_LEVEL selected_level{};
            constexpr D3D_FEATURE_LEVEL levels[] = {
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
            };

            HRESULT result = ::D3D11CreateDevice(
                nullptr,
                D3D_DRIVER_TYPE_HARDWARE,
                nullptr,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                levels,
                ARRAYSIZE(levels),
                D3D11_SDK_VERSION,
                d3d_device.put(),
                &selected_level,
                d3d_context.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D3D device failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<IDXGIDevice> dxgi_device;
            result = d3d_device->QueryInterface(__uuidof(IDXGIDevice), dxgi_device.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame DXGI device failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<IDXGIAdapter> adapter;
            result = dxgi_device->GetAdapter(adapter.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame DXGI adapter failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<IDXGIFactory2> factory;
            result = adapter->GetParent(__uuidof(IDXGIFactory2), factory.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame DXGI factory failed: " + HresultText(result);
                return;
            }

            DXGI_SWAP_CHAIN_DESC1 desc{};
            desc.Width = static_cast<UINT>(width);
            desc.Height = static_cast<UINT>(height);
            desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
            desc.Stereo = false;
            desc.SampleDesc.Count = 1;
            desc.SampleDesc.Quality = 0;
            desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
            desc.BufferCount = 2;
            desc.Scaling = DXGI_SCALING_STRETCH;
            desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
            desc.AlphaMode = DXGI_ALPHA_MODE_IGNORE;

            winrt::com_ptr<IDXGISwapChain1> swap_chain;
            result = factory->CreateSwapChainForComposition(
                d3d_device.get(),
                &desc,
                nullptr,
                swap_chain.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame swap-chain creation failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<ID3D11Texture2D> back_buffer;
            result = swap_chain->GetBuffer(0, __uuidof(ID3D11Texture2D), back_buffer.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame back-buffer failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<IDXGISurface> dxgi_surface;
            result = back_buffer->QueryInterface(__uuidof(IDXGISurface), dxgi_surface.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame DXGI surface failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<ID2D1Factory3> d2d_factory;
            D2D1_FACTORY_OPTIONS options{};
            result = ::D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                __uuidof(ID2D1Factory3),
                &options,
                reinterpret_cast<void**>(d2d_factory.put()));
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D factory failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<ID2D1Device> d2d_device;
            result = d2d_factory->CreateDevice(dxgi_device.get(), d2d_device.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D device failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<ID2D1DeviceContext> d2d_context;
            result = d2d_device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, d2d_context.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D context failed: " + HresultText(result);
                return;
            }

            const auto bitmap_properties = D2D1::BitmapProperties1(
                D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE),
                96.0f,
                96.0f);
            winrt::com_ptr<ID2D1Bitmap1> target_bitmap;
            result = d2d_context->CreateBitmapFromDxgiSurface(
                dxgi_surface.get(),
                &bitmap_properties,
                target_bitmap.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D target failed: " + HresultText(result);
                return;
            }

            d2d_context->SetTarget(target_bitmap.get());
            d2d_context->BeginDraw();
            d2d_context->Clear(D2D1::ColorF(0.04f, 0.10f, 0.14f, 1.0f));

            winrt::com_ptr<IDWriteFactory> dwrite_factory;
            result = ::DWriteCreateFactory(
                DWRITE_FACTORY_TYPE_SHARED,
                __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown**>(dwrite_factory.put()));
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap label factory failed: " + HresultText(result);
                return;
            }

            winrt::com_ptr<IDWriteTextFormat> label_format;
            result = dwrite_factory->CreateTextFormat(
                L"Segoe UI",
                nullptr,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.0f,
                L"",
                label_format.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap label format failed: " + HresultText(result);
                return;
            }
            label_format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            label_format->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            label_format->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);

            struct DrawTile
            {
                float left;
                float top;
                float right;
                float bottom;
                D2D1_COLOR_F color;
                std::wstring label;
            };

            struct TileInput
            {
                double weight;
                D2D1_COLOR_F color;
                TreeCatalogEntry entry;
            };

            const D2D1_COLOR_F palette[] = {
                D2D1::ColorF(0.10f, 0.36f, 0.67f, 1.0f),
                D2D1::ColorF(0.13f, 0.50f, 0.35f, 1.0f),
                D2D1::ColorF(0.43f, 0.25f, 0.62f, 1.0f),
                D2D1::ColorF(0.70f, 0.36f, 0.11f, 1.0f),
                D2D1::ColorF(0.60f, 0.12f, 0.16f, 1.0f),
                D2D1::ColorF(0.28f, 0.46f, 0.52f, 1.0f),
            };

            std::vector<TileInput> tile_inputs;
            tile_inputs.reserve(10);
            for (auto const& entry : m_tree_catalog) {
                if (tile_inputs.size() >= 10) {
                    break;
                }
                const double weight = entry.size_bytes > 0
                    ? static_cast<double>(entry.size_bytes)
                    : static_cast<double>((std::max)(1, entry.progress));
                tile_inputs.push_back(TileInput{ weight, palette[tile_inputs.size() % ARRAYSIZE(palette)], entry });
            }
            std::sort(tile_inputs.begin(), tile_inputs.end(), [](TileInput const& left, TileInput const& right) {
                return left.weight > right.weight;
            });

            const float surface_width = static_cast<float>(width);
            const float surface_height = static_cast<float>(height);
            const float gap = 4.0f;
            std::vector<DrawTile> tiles;
            tiles.reserve(tile_inputs.size());
            std::vector<TreemapTileLayout> layout;
            layout.reserve(tile_inputs.size());

            struct LayoutNode
            {
                size_t begin;
                size_t end;
                float left;
                float top;
                float right;
                float bottom;
            };

            auto range_weight = [&](size_t begin, size_t end) {
                double total = 0.0;
                for (size_t index = begin; index < end; ++index) {
                    total += (std::max)(1.0, tile_inputs[index].weight);
                }
                return total;
            };

            std::vector<LayoutNode> pending_layout;
            pending_layout.push_back(LayoutNode{ 0, tile_inputs.size(), gap, gap, surface_width - gap, surface_height - gap });
            while (!pending_layout.empty()) {
                const LayoutNode node = pending_layout.back();
                pending_layout.pop_back();
                if (node.begin >= node.end) {
                    continue;
                }

                const float node_width = node.right - node.left;
                const float node_height = node.bottom - node.top;
                if (node.end - node.begin == 1 || node_width < 32.0f || node_height < 24.0f) {
                    auto const& input = tile_inputs[node.begin];
                    const float left = node.left;
                    const float top = node.top;
                    const float right = (std::max)(left + 2.0f, node.right);
                    const float bottom = (std::max)(top + 2.0f, node.bottom);
                    tiles.push_back(DrawTile{ left, top, right, bottom, input.color, input.entry.name });
                    layout.push_back(TreemapTileLayout{
                        left,
                        top,
                        right,
                        bottom,
                        input.entry.name,
                        input.entry.path,
                        input.entry.kind,
                        input.entry.size_text,
                    });
                    continue;
                }

                const double total = range_weight(node.begin, node.end);
                double prefix = 0.0;
                size_t split = node.begin + 1;
                for (; split + 1 < node.end; ++split) {
                    prefix += (std::max)(1.0, tile_inputs[split - 1].weight);
                    if (prefix >= total * 0.5) {
                        break;
                    }
                }

                const double leading_weight = range_weight(node.begin, split);
                const float ratio = static_cast<float>(leading_weight / (std::max)(1.0, total));
                if (node_width >= node_height) {
                    const float split_x = std::clamp(
                        node.left + (node_width * ratio),
                        node.left + gap + 2.0f,
                        node.right - gap - 2.0f);
                    pending_layout.push_back(LayoutNode{ split, node.end, split_x + gap, node.top, node.right, node.bottom });
                    pending_layout.push_back(LayoutNode{ node.begin, split, node.left, node.top, split_x - gap, node.bottom });
                } else {
                    const float split_y = std::clamp(
                        node.top + (node_height * ratio),
                        node.top + gap + 2.0f,
                        node.bottom - gap - 2.0f);
                    pending_layout.push_back(LayoutNode{ split, node.end, node.left, split_y + gap, node.right, node.bottom });
                    pending_layout.push_back(LayoutNode{ node.begin, split, node.left, node.top, node.right, split_y - gap });
                }
            }

            for (auto const& tile : tiles) {
                winrt::com_ptr<ID2D1SolidColorBrush> brush;
                result = d2d_context->CreateSolidColorBrush(tile.color, brush.put());
                if (FAILED(result)) {
                    m_treemap_render_status = L"Treemap probe frame D2D brush failed: " + HresultText(result);
                    return;
                }
                d2d_context->FillRectangle(
                    D2D1::RectF(tile.left, tile.top, tile.right, tile.bottom),
                    brush.get());
            }

            winrt::com_ptr<ID2D1SolidColorBrush> label_brush;
            result = d2d_context->CreateSolidColorBrush(D2D1::ColorF(1.0f, 1.0f, 1.0f, 0.92f), label_brush.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap label brush failed: " + HresultText(result);
                return;
            }

            size_t labels_drawn = 0;
            for (auto const& tile : tiles) {
                const float tile_width = tile.right - tile.left;
                const float tile_height = tile.bottom - tile.top;
                if (tile_width < 64.0f || tile_height < 24.0f || tile.label.empty()) {
                    continue;
                }

                d2d_context->DrawTextW(
                    tile.label.c_str(),
                    static_cast<UINT32>(tile.label.size()),
                    label_format.get(),
                    D2D1::RectF(tile.left + 8.0f, tile.top + 7.0f, tile.right - 8.0f, tile.bottom - 6.0f),
                    label_brush.get(),
                    D2D1_DRAW_TEXT_OPTIONS_CLIP);
                ++labels_drawn;
            }

            result = d2d_context->EndDraw();
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D draw failed: " + HresultText(result);
                return;
            }

            result = swap_chain->Present(1, 0);
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame present failed: " + HresultText(result);
                return;
            }

            auto panel_native = TreemapSurface().as<ISwapChainPanelNative>();
            result = panel_native->SetSwapChain(swap_chain.get());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame panel bind failed: " + HresultText(result);
                return;
            }

            const int level_major = static_cast<int>((selected_level >> 12) & 0xF);
            const int level_minor = static_cast<int>((selected_level >> 8) & 0xF);
            m_treemap_probe_frame_rendered = true;
            m_treemap_render_dirty = false;
            m_treemap_render_width = width;
            m_treemap_render_height = height;
            m_treemap_tile_layout = std::move(layout);
            if (m_treemap_tile_layout.empty()) {
                m_treemap_render_status = L"GPU treemap catalog frame rendered: no catalog tiles yet; scan or load a cached catalog.";
                return;
            }
            std::wstring first_tile = m_treemap_tile_layout.empty() ? L"none" : m_treemap_tile_layout.front().name;
            m_treemap_render_status = L"GPU treemap catalog frame rendered: " +
                std::to_wstring(tile_inputs.size()) + L" tiles, D3D feature level " +
                std::to_wstring(level_major) + L"." + std::to_wstring(level_minor) +
                L"; layout=balanced, labels=" + std::to_wstring(labels_drawn) +
                L", first tile=\"" + first_tile + L"\".";
        }
        catch (winrt::hresult_error const& error) {
            m_treemap_render_status = L"Treemap probe frame failed: " + std::wstring(error.message().c_str());
        }
    }

    void MainWindow::OnTreemapSurfacePointerMoved(
        winrt::Windows::Foundation::IInspectable const& sender,
        Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& args)
    {
        auto surface = sender.try_as<Microsoft::UI::Xaml::UIElement>();
        if (!surface || m_treemap_tile_layout.empty()) {
            return;
        }

        const auto point = args.GetCurrentPoint(surface).Position();
        for (auto const& tile : m_treemap_tile_layout) {
            if (point.X >= tile.left && point.X <= tile.right &&
                point.Y >= tile.top && point.Y <= tile.bottom) {
                m_hovered_treemap_name = tile.name;
                m_hovered_treemap_path = tile.path;
                m_hovered_treemap_kind = tile.kind;
                m_hovered_treemap_size = tile.size_text;
                if (TreemapZoomOverlay()) {
                    TreemapZoomOverlay().Visibility(Microsoft::UI::Xaml::Visibility::Visible);
                }
                UpdateTreemapFocus(tile.name, tile.path, tile.kind, tile.size_text);
                UpdateEventText(L"Hovering GPU tile: " + tile.name);
                return;
            }
        }

        if (TreemapZoomOverlay()) {
            TreemapZoomOverlay().Visibility(Microsoft::UI::Xaml::Visibility::Collapsed);
        }
    }

    void MainWindow::OnTreemapSurfacePointerExited(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&)
    {
        if (TreemapZoomOverlay()) {
            TreemapZoomOverlay().Visibility(Microsoft::UI::Xaml::Visibility::Collapsed);
        }
        UpdateEventText(L"Selected " + CurrentVisualizationLabel());
    }

    void MainWindow::OnTreemapSurfaceTapped(
        winrt::Windows::Foundation::IInspectable const& sender,
        Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const& args)
    {
        auto surface = sender.try_as<Microsoft::UI::Xaml::UIElement>();
        if (!surface || m_treemap_tile_layout.empty()) {
            return;
        }

        const auto point = args.GetPosition(surface);
        for (auto const& tile : m_treemap_tile_layout) {
            if (point.X >= tile.left && point.X <= tile.right &&
                point.Y >= tile.top && point.Y <= tile.bottom) {
                SelectVisualizationTarget(tile.name, tile.path, tile.kind, tile.size_text);
                NavigateToSection(ShellSection::Treemap);
                UpdateEventText(L"Selected GPU tile: " + tile.name);
                return;
            }
        }
    }

    void MainWindow::SetSection(ShellSection section)
    {
        TraceStartup(L"SetSection begin");
        m_active_section = section;
        if (!SectionText()) {
            TraceStartup(L"SetSection skipped: section text unavailable");
            return;
        }
        SectionText().Text(winrt::hstring(SectionName(section)));
        UpdateNavigationChips();
        if (SectionMenu()) {
            m_section_menu_updates_suppressed = true;
            SectionMenu().SelectedIndex(SectionMenuIndex(section));
            m_section_menu_updates_suppressed = false;
        }
        UpdateBreadcrumbs();
        UpdateSummaryText();

        if (m_session_active) {
            TraceStartup(L"SetSection visibility deferred during active scan");
            TraceStartup(L"SetSection end");
            return;
        }

        SetControlVisibility(OverviewCard(), m_show_current_state && (section == ShellSection::Overview || section == ShellSection::Diagnostics));
        SetControlVisibility(TreeCard(), m_show_folder_tree && (section == ShellSection::Overview || section == ShellSection::Tree));
        SetControlVisibility(SearchCard(), section == ShellSection::Overview || section == ShellSection::Search);
        SetControlVisibility(DiagnosticsCard(), m_show_runtime_metrics && (section == ShellSection::Overview || section == ShellSection::Diagnostics));
        SetControlVisibility(TreemapCard(), true);
        SetControlVisibility(DetailCard(), m_show_folder_view && (section == ShellSection::Overview || section == ShellSection::Treemap || section == ShellSection::Diagnostics));
        TraceStartup(L"SetSection end");
    }

    void MainWindow::UpdateNavigationChips()
    {
        const std::wstring active = SectionName(m_active_section);
        for (auto const& chip : m_nav_chips) {
            if (!chip) {
                continue;
            }

            const std::wstring section = winrt::unbox_value_or<winrt::hstring>(chip.Tag(), L"").c_str();
            ApplyNavigationChipStyle(chip, section == active);
        }
    }

    int MainWindow::SectionMenuIndex(ShellSection section) const
    {
        switch (section) {
        case ShellSection::Overview:
            return 0;
        case ShellSection::Tree:
            return 1;
        case ShellSection::Treemap:
            return 2;
        case ShellSection::Search:
            return 3;
        case ShellSection::Diagnostics:
            return 4;
        default:
            return 0;
        }
    }

    ShellSection MainWindow::SectionFromMenuIndex(int index) const
    {
        switch (index) {
        case 1:
            return ShellSection::Tree;
        case 2:
            return ShellSection::Treemap;
        case 3:
            return ShellSection::Search;
        case 4:
            return ShellSection::Diagnostics;
        case 0:
        default:
            return ShellSection::Overview;
        }
    }

    Microsoft::UI::Xaml::Media::SolidColorBrush MainWindow::MakeBrush(Windows::UI::Color const& color) const
    {
        Microsoft::UI::Xaml::Media::SolidColorBrush brush;
        brush.Color(color);
        return brush;
    }

    void MainWindow::ApplyCardStyle(Microsoft::UI::Xaml::Controls::Border const& card) const
    {
        auto const& theme = ActiveShellTheme();
        card.CornerRadius(UniformRadius(theme.card_radius));
        card.Padding(UniformThickness(theme.card_padding));
        card.Background(MakeBrush(theme.card_background));
        card.BorderBrush(MakeBrush(theme.card_border));
        card.BorderThickness(UniformThickness(1.0));
    }

    void MainWindow::ApplyCompactCardStyle(Microsoft::UI::Xaml::Controls::Border const& card) const
    {
        auto const& theme = ActiveShellTheme();
        card.CornerRadius(UniformRadius(theme.compact_card_radius));
        card.Padding(Microsoft::UI::Xaml::Thickness{ 12.0, 10.0, 12.0, 10.0 });
        card.Background(MakeBrush(theme.card_background));
        card.BorderBrush(MakeBrush(theme.card_border));
        card.BorderThickness(UniformThickness(1.0));
    }

    void MainWindow::ApplyAccentPanelStyle(
        Microsoft::UI::Xaml::Controls::Border const& panel,
        Windows::UI::Color const& background,
        Windows::UI::Color const& border) const
    {
        auto const& theme = ActiveShellTheme();
        panel.CornerRadius(UniformRadius(theme.panel_radius));
        panel.Padding(UniformThickness(12.0));
        panel.Background(MakeBrush(background));
        panel.BorderBrush(MakeBrush(border));
        panel.BorderThickness(UniformThickness(1.0));
    }

    Microsoft::UI::Xaml::Controls::TextBlock MainWindow::MakeCardTitle(std::wstring_view text) const
    {
        auto const& theme = ActiveShellTheme();
        auto title = Microsoft::UI::Xaml::Controls::TextBlock{};
        title.Text(winrt::hstring(text));
        title.FontSize(theme.card_title_size);
        title.Foreground(MakeBrush(theme.text_primary));
        return title;
    }

    void MainWindow::ApplyNavigationChipStyle(Microsoft::UI::Xaml::Controls::Button const& chip, bool active) const
    {
        auto const& theme = ActiveShellTheme();
        chip.CornerRadius(UniformRadius(theme.chip_radius));
        chip.Padding(Microsoft::UI::Xaml::Thickness{ 12.0, 6.0, 12.0, 6.0 });
        chip.Background(MakeBrush(active ? theme.chip_active_background : theme.chip_inactive_background));
        chip.BorderBrush(MakeBrush(active ? theme.chip_active_border : Windows::UI::Colors::Transparent()));
        chip.BorderThickness(active
            ? UniformThickness(1.0)
            : UniformThickness(0.0));
    }

    void MainWindow::ApplyShellState()
    {
        TraceStartup(L"ApplyShellState begin");
        const bool loading = !m_shell_ready;
        SetControlVisibility(LoadingBanner(), loading);
        SetControlVisibility(EmptyBanner(), m_shell_ready && !m_session_active && !m_has_results && !m_has_error);
        SetControlVisibility(ScanningBanner(), m_session_active);
        SetControlVisibility(ErrorBanner(), m_has_error);
        if (!m_session_active) {
            SetSection(m_active_section);
        }
        UpdateSummaryText();
        UpdateRuntimeSnapshot();
        TraceStartup(L"ApplyShellState end");
    }

    void MainWindow::ScheduleUiFlush()
    {
        bool should_start = false;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            should_start = !m_ui_flush_requested;
            m_ui_flush_requested = true;
        }

        if (should_start) {
            if (!m_ui_flush_timer) {
                TraceStartup(L"ScheduleUiFlush skipped: timer unavailable");
                return;
            }
            TraceStartup(L"ScheduleUiFlush requested");
            DispatcherQueue().TryEnqueue([this]() {
                TraceStartup(L"ScheduleUiFlush timer started");
                m_ui_flush_timer.Start();
            });
        }
    }

    void MainWindow::ScheduleTreemapRender(std::wstring const& reason)
    {
        if (!TreemapSurface()) {
            return;
        }

        ++m_total_treemap_render_request_count;
        if (m_treemap_render_requested) {
            ++m_total_treemap_render_coalesced_count;
        }
        m_treemap_render_requested = true;
        if (!m_treemap_render_timer) {
            m_treemap_render_timer = Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread().CreateTimer();
            m_treemap_render_timer.Interval(std::chrono::milliseconds(33));
            m_treemap_render_timer.IsRepeating(false);
            m_treemap_render_timer.Tick([this](auto const&, auto const&) {
                m_treemap_render_requested = false;
                ++m_total_treemap_render_flush_count;
                if (!TreemapSurface()) {
                    return;
                }

                const int width = (std::max)(1, static_cast<int>(TreemapSurface().ActualWidth()));
                const int height = (std::max)(1, static_cast<int>(TreemapSurface().ActualHeight()));
                RenderTreemapProbeFrame(width, height);
                if (TreemapSurfaceStatusText()) {
                    TreemapSurfaceStatusText().Text(winrt::hstring(
                        L"SwapChainPanel host active: " +
                        std::to_wstring(width) + L"x" +
                        std::to_wstring(height) +
                        L" px. " + m_treemap_render_status));
                }
                m_last_composition_frame_time = std::chrono::steady_clock::now();
                UpdatePerformanceCounters(L"treemap render flush");
            });
        }

        TraceStartup(L"ScheduleTreemapRender requested: " + reason);
        DispatcherQueue().TryEnqueue([this]() {
            if (m_treemap_render_timer) {
                m_treemap_render_timer.Stop();
                m_treemap_render_timer.Start();
            }
        });
    }

    void MainWindow::FlushPendingUiState()
    {
        TraceStartup(L"FlushPendingUiState begin");
        PendingUiState pending;
        bool has_pending = false;
        const auto flush_started = std::chrono::steady_clock::now();

        {
            std::lock_guard guard(m_pending_ui_mutex);
            has_pending = m_pending_ui_state.status_dirty || m_pending_ui_state.event_dirty ||
                m_pending_ui_state.summary_dirty || m_pending_ui_state.progress_dirty ||
                m_pending_ui_state.error_dirty || m_pending_ui_state.selection_dirty ||
                m_pending_ui_state.visualization_dirty || m_pending_ui_state.catalog_dirty;

            if (has_pending) {
                pending = std::move(m_pending_ui_state);
                m_pending_ui_state = {};
            }

            m_ui_flush_requested = false;
            m_ui_flush_timer.Stop();
        }

        if (!has_pending) {
            TraceStartup(L"FlushPendingUiState no pending work");
            return;
        }

        if (pending.status_dirty) {
            UpdateStatus(pending.status_text);
        }

        if (pending.event_dirty) {
            UpdateEventText(pending.event_text);
        }

        if (pending.error_dirty && !pending.error_text.empty()) {
            ErrorText().Text(winrt::hstring(pending.error_text));
        }

        if (pending.summary_dirty) {
            SummaryText().Text(winrt::hstring(pending.summary_text));
        }

        if (pending.progress_dirty) {
            UpdateProgress(pending.progress_percent, pending.progress_text);
        }

        if (pending.selection_dirty) {
            SelectVisualizationTarget(
                pending.selected_name,
                pending.selected_path,
                pending.selected_kind,
                pending.selected_size);
        }

        if (pending.visualization_dirty) {
            UpdateTreemapFocus(
                pending.treemap_hover_name,
                pending.treemap_hover_path,
                pending.treemap_hover_kind,
                pending.treemap_hover_size);
        }

        if (pending.diagnostics_dirty) {
            m_last_progress_items_done = pending.progress_items_done;
            m_last_progress_items_total = pending.progress_items_total;
            m_last_progress_bytes_done = pending.progress_bytes_done;
            m_last_progress_bytes_total = pending.progress_bytes_total;
            m_last_summary_files_seen = pending.summary_files_seen;
            m_last_summary_directories_seen = pending.summary_directories_seen;
            m_last_summary_total_size_bytes = pending.summary_total_size_bytes;
        }

        if (pending.correctness_dirty) {
            if (pending.reset_scan_issues) {
                m_scan_issue_count = 0;
                m_last_scan_issue_text = L"none";
                m_recent_scan_issues.clear();
                m_scan_issue_code_counts.clear();
                m_incremental_added = 0;
                m_incremental_removed = 0;
                m_incremental_modified = 0;
                m_incremental_renamed = 0;
                m_incremental_moved = 0;
            }
            m_scan_issue_count += pending.issue_count_delta;
            for (auto const& [code, count] : pending.issue_code_deltas) {
                m_scan_issue_code_counts[code] += count;
            }
            if (pending.incremental_changes_dirty) {
                m_incremental_added = pending.incremental_added;
                m_incremental_removed = pending.incremental_removed;
                m_incremental_modified = pending.incremental_modified;
                m_incremental_renamed = pending.incremental_renamed;
                m_incremental_moved = pending.incremental_moved;
            }
            if (!pending.last_issue_text.empty()) {
                m_last_scan_issue_text = pending.last_issue_text;
            }
            for (auto const& issue : pending.recent_issue_texts) {
                if (!issue.empty()) {
                    m_recent_scan_issues.push_back(issue);
                    if (m_recent_scan_issues.size() > kRecentIssueLimit) {
                        m_recent_scan_issues.erase(m_recent_scan_issues.begin());
                    }
                }
            }
        }

        if (pending.catalog_dirty && !pending.catalog_entries.empty()) {
            for (auto const& entry : pending.catalog_entries) {
                if (m_tree_catalog_keys.insert(TreeCatalogKey(entry)).second) {
                    m_tree_catalog.push_back(entry);
                }
            }
        }

        if (pending.reload_snapshot) {
            if (m_session_active) {
                TraceStartup(L"FlushPendingUiState snapshot reload deferred during active scan");
            } else {
                LoadPersistedCatalogSnapshot();
            }
        } else if (pending.catalog_dirty && !pending.catalog_entries.empty()) {
            UpdateTreeSnapshotPreview(FilterTreeCatalog());
            UpdateCatalogSnapshot();
            m_treemap_render_dirty = true;
            ScheduleTreemapRender(L"catalog flush");
        }

        m_last_ui_flush_time = flush_started;
        m_last_working_set_bytes = CurrentWorkingSetBytes();
        m_peak_working_set_bytes = (std::max)(m_peak_working_set_bytes, m_last_working_set_bytes);
        ++m_total_ui_flush_count;
        if (m_last_ui_event_time.time_since_epoch().count() > 0) {
            const auto latency = std::chrono::duration<double, std::milli>(flush_started - m_last_ui_event_time);
            m_last_ui_latency_ms = latency.count();
        }
        m_last_ui_flush_duration_ms = std::chrono::duration<double, std::milli>(
            std::chrono::steady_clock::now() - flush_started).count();
        m_peak_ui_flush_duration_ms = (std::max)(m_peak_ui_flush_duration_ms, m_last_ui_flush_duration_ms);
        m_last_composition_frame_time = std::chrono::steady_clock::now();

        UpdatePerformanceCounters(L"batched flush");
        UpdateCorrectnessDiagnostics();
        UpdateRecentIssueDiagnostics();
        TraceStartup(L"FlushPendingUiState end");
    }

    void MainWindow::UpdatePerformanceCounters(std::wstring const& reason)
    {
        if (!PerformanceText()) {
            return;
        }
        double elapsed_seconds = 0.0;
        if (m_scan_started_at.time_since_epoch().count() > 0) {
            elapsed_seconds = std::chrono::duration<double>(
                std::chrono::steady_clock::now() - m_scan_started_at).count();
        }
        const double items_per_second = elapsed_seconds > 0.0
            ? static_cast<double>(m_last_progress_items_done) / elapsed_seconds
            : 0.0;
        const double bytes_per_second = elapsed_seconds > 0.0
            ? static_cast<double>(m_last_progress_bytes_done) / elapsed_seconds
            : 0.0;

        const std::wstring text =
            L"UI batching: " + reason +
            L", flushes=" + std::to_wstring(m_total_ui_flush_count) +
            L", queued events=" + std::to_wstring(m_pending_event_count) +
            L", last latency=" + std::to_wstring(static_cast<int>(m_last_ui_latency_ms)) + L" ms" +
            L", last input=" + std::to_wstring(static_cast<int>(m_last_input_latency_ms)) + L" ms" +
            L", flush cost=" + std::to_wstring(static_cast<int>(m_last_ui_flush_duration_ms)) + L" ms" +
            L", peak flush=" + std::to_wstring(static_cast<int>(m_peak_ui_flush_duration_ms)) + L" ms" +
            L", frames=" + std::to_wstring(m_total_composition_frame_count) +
            L", last frame=" + std::to_wstring(static_cast<int>(m_last_composition_frame_ms)) + L" ms" +
            L", peak frame=" + std::to_wstring(static_cast<int>(m_peak_composition_frame_ms)) + L" ms" +
            L", treemap renders=" + std::to_wstring(m_total_treemap_render_flush_count) +
            L"/" + std::to_wstring(m_total_treemap_render_request_count) +
            L" requests, coalesced=" + std::to_wstring(m_total_treemap_render_coalesced_count) +
            L"\nScan throughput: " + std::to_wstring(static_cast<int>(items_per_second)) + L" items/s, " +
            FormatBytes(static_cast<uint64_t>(bytes_per_second)) + L"/s" +
            L", progress=" + std::to_wstring(m_last_progress_items_done) + L"/" +
            std::to_wstring(m_last_progress_items_total) +
            L", bytes=" + FormatBytes(m_last_progress_bytes_done) + L"/" +
            FormatBytes(m_last_progress_bytes_total) +
            L"\nMemory: working set=" + FormatBytes(m_last_working_set_bytes) +
            L", peak=" + FormatBytes(m_peak_working_set_bytes) +
            L", summary=" + std::to_wstring(m_last_summary_files_seen) + L" files, " +
            std::to_wstring(m_last_summary_directories_seen) + L" directories, " +
            FormatBytes(m_last_summary_total_size_bytes) +
            L"\n" + m_last_cache_load_text;
        PerformanceText().Text(winrt::hstring(text));
        UpdateRuntimeSnapshot();
    }

    void MainWindow::UpdateSummaryText()
    {
        if (!SummaryText()) {
            return;
        }

        const std::wstring text =
            L"Root path: " + m_current_root_path +
            L" | Section: " + SectionName(m_active_section) +
            L" | Selection: " + m_current_selection_name +
            L" (" + m_current_selection_kind + L")" +
            L" | State: " + std::wstring(m_session_active ? L"scanning" : L"idle") +
            L" | Results: " + std::wstring(m_has_results ? L"loaded" : L"none") +
            L" | " + m_last_scan_duration_text +
            L" | Error: " + std::wstring(m_has_error ? L"yes" : L"no");

        SummaryText().Text(winrt::hstring(text));
    }

    void MainWindow::UpdateRuntimeSnapshot()
    {
        if (!RuntimeSnapshotText()) {
            return;
        }

        const std::wstring text =
            L"UI batching: " + std::wstring(m_shell_ready ? L"ready" : L"starting") +
            L", flushes=" + std::to_wstring(m_total_ui_flush_count) +
            L", queued events=" + std::to_wstring(m_pending_event_count) +
            L", last latency=" + std::to_wstring(static_cast<int>(m_last_ui_latency_ms)) + L" ms" +
            L", last input=" + std::to_wstring(static_cast<int>(m_last_input_latency_ms)) + L" ms" +
            L", flush cost=" + std::to_wstring(static_cast<int>(m_last_ui_flush_duration_ms)) + L" ms" +
            L", peak flush=" + std::to_wstring(static_cast<int>(m_peak_ui_flush_duration_ms)) + L" ms" +
            L", frames=" + std::to_wstring(m_total_composition_frame_count) +
            L", last frame=" + std::to_wstring(static_cast<int>(m_last_composition_frame_ms)) + L" ms" +
            L", peak frame=" + std::to_wstring(static_cast<int>(m_peak_composition_frame_ms)) + L" ms" +
            L", treemap renders=" + std::to_wstring(m_total_treemap_render_flush_count) +
            L"/" + std::to_wstring(m_total_treemap_render_request_count) +
            L" requests, coalesced=" + std::to_wstring(m_total_treemap_render_coalesced_count) +
            L", session=" + std::wstring(m_session_active ? L"active" : L"idle") +
            L", results=" + std::wstring(m_has_results ? L"loaded" : L"none") +
            L", working set=" + FormatBytes(m_last_working_set_bytes) +
            L", peak=" + FormatBytes(m_peak_working_set_bytes) +
            L", " + m_last_cache_load_text +
            L", " + m_last_scan_duration_text;

        RuntimeSnapshotText().Text(winrt::hstring(text));
        UpdateCorrectnessDiagnostics();
        UpdateRecentIssueDiagnostics();
    }

    void MainWindow::UpdateCorrectnessDiagnostics()
    {
        if (!CorrectnessText()) {
            return;
        }

        uint64_t catalog_known_bytes = 0;
        size_t catalog_sized_entries = 0;
        for (auto const& entry : m_tree_catalog) {
            if (entry.size_bytes > 0 || entry.size_text == L"0 B") {
                catalog_known_bytes += entry.size_bytes;
                ++catalog_sized_entries;
            }
        }

        std::wstring reconciliation = L"summary pending";
        if (m_last_summary_total_size_bytes > 0 ||
            m_last_summary_files_seen > 0 ||
            m_last_summary_directories_seen > 0) {
            const uint64_t larger = (std::max)(m_last_summary_total_size_bytes, catalog_known_bytes);
            const uint64_t smaller = (std::min)(m_last_summary_total_size_bytes, catalog_known_bytes);
            reconciliation = L"summary=" + FormatBytes(m_last_summary_total_size_bytes) +
                L", catalog sample=" + FormatBytes(catalog_known_bytes) +
                L", delta=" + FormatBytes(larger - smaller);
        }

        std::wstring issue_breakdown = L"none";
        if (!m_scan_issue_code_counts.empty()) {
            issue_breakdown.clear();
            size_t emitted = 0;
            for (auto const& [code, count] : m_scan_issue_code_counts) {
                if (emitted > 0) {
                    issue_breakdown += L" | ";
                }
                issue_breakdown += IssueCodeLabel(code) + L" code " + std::to_wstring(code) + L": " + std::to_wstring(count);
                ++emitted;
                if (emitted == 4 && m_scan_issue_code_counts.size() > emitted) {
                    issue_breakdown += L" | more";
                    break;
                }
            }
        }

        const std::wstring text =
            L"Correctness: issues=" + std::to_wstring(m_scan_issue_count) +
            L", issue codes=" + issue_breakdown +
            L", incremental added=" + std::to_wstring(m_incremental_added) +
            L", removed=" + std::to_wstring(m_incremental_removed) +
            L", modified=" + std::to_wstring(m_incremental_modified) +
            L", renamed=" + std::to_wstring(m_incremental_renamed) +
            L", moved=" + std::to_wstring(m_incremental_moved) +
            L", last issue=" + m_last_scan_issue_text +
            L", files=" + std::to_wstring(m_last_summary_files_seen) +
            L", directories=" + std::to_wstring(m_last_summary_directories_seen) +
            L", sized catalog entries=" + std::to_wstring(catalog_sized_entries) +
            L", totals " + reconciliation;

        CorrectnessText().Text(winrt::hstring(text));
    }

    void MainWindow::UpdateRecentIssueDiagnostics()
    {
        if (!RecentIssuesText()) {
            return;
        }

        if (m_recent_scan_issues.empty()) {
            RecentIssuesText().Text(L"Recent issues: none");
        } else {
            std::wstring text = L"Recent issues:";
            for (size_t index = 0; index < m_recent_scan_issues.size(); ++index) {
                text += L" ";
                text += std::to_wstring(index + 1);
                text += L") ";
                text += m_recent_scan_issues[index];
            }
            RecentIssuesText().Text(winrt::hstring(text));
        }

        if (IssueDrilldownText()) {
            const auto code_count = [&](uint32_t code) -> uint64_t {
                auto found = m_scan_issue_code_counts.find(code);
                return found == m_scan_issue_code_counts.end() ? 0 : found->second;
            };
            const uint64_t permissions = code_count(10);
            const uint64_t missing = code_count(11);
            const uint64_t sharing = code_count(12);
            const uint64_t transient = code_count(13);
            const uint64_t unsupported = code_count(14);
            const uint64_t skipped = permissions + missing + sharing + unsupported;

            std::wstring drilldown =
                L"Issue drill-down: errors=" + std::to_wstring(m_scan_issue_count) +
                L", skipped=" + std::to_wstring(skipped) +
                L", transient=" + std::to_wstring(transient) +
                L", permissions=" + std::to_wstring(permissions) +
                L", missing=" + std::to_wstring(missing) +
                L", sharing=" + std::to_wstring(sharing) +
                L", unsupported=" + std::to_wstring(unsupported) +
                L", last=" + m_last_scan_issue_text;
            IssueDrilldownText().Text(winrt::hstring(drilldown));
        }
    }

    std::wstring MainWindow::Utf8ToWide(std::string_view text)
    {
        if (text.empty()) {
            return {};
        }

        const int required = MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text.data(),
            static_cast<int>(text.size()),
            nullptr,
            0);
        if (required <= 0) {
            return {};
        }

        std::wstring output(static_cast<size_t>(required), L'\0');
        MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text.data(),
            static_cast<int>(text.size()),
            output.data(),
            required);
        return output;
    }

    MainWindow::TreeCatalogEntry MainWindow::CatalogEntryFromNative(WbCatalogEntry const& entry) const
    {
        auto to_wide = [this](WbCStringView view) {
            if (view.ptr == nullptr || view.len == 0) {
                return std::wstring{};
            }
            return Utf8ToWide(std::string_view{ view.ptr, view.len });
        };

        TreeCatalogEntry catalog_entry;
        catalog_entry.name = to_wide(entry.name);
        catalog_entry.path = to_wide(entry.path);
        catalog_entry.kind = to_wide(entry.kind);
        catalog_entry.size_text = to_wide(entry.size_text);
        catalog_entry.size_bytes = entry.size_bytes;
        catalog_entry.description = to_wide(entry.description);
        if (entry.has_modified_utc) {
            catalog_entry.modified_utc = static_cast<int64_t>(entry.modified_utc);
        }
        catalog_entry.progress = catalog_entry.kind == L"Volume" ? 100 : 0;
        {
            // Build search_text_lower with a single pre-sized allocation and an in-place
            // lowercase transform.  The previous chained operator+ approach created 6
            // temporary wstrings before LowercaseCopy made a 7th copy; for a catalog load
            // cap of 8,192 entries that was ~57,344 unnecessary heap allocations.
            std::wstring search_text;
            search_text.reserve(
                catalog_entry.name.size() + 1 +
                catalog_entry.path.size() + 1 +
                catalog_entry.kind.size() + 1 +
                catalog_entry.description.size());
            search_text.append(catalog_entry.name);
            search_text += L'\n';
            search_text.append(catalog_entry.path);
            search_text += L'\n';
            search_text.append(catalog_entry.kind);
            search_text += L'\n';
            search_text.append(catalog_entry.description);
            std::transform(search_text.begin(), search_text.end(), search_text.begin(),
                [](wchar_t ch) { return static_cast<wchar_t>(std::towlower(ch)); });
            catalog_entry.search_text_lower = std::move(search_text);
        }
        catalog_entry.extension_lower = ExtensionLower(catalog_entry.path);
        catalog_entry.path_depth = PathDepth(catalog_entry.path);
        catalog_entry.parent_path = ParentPath(catalog_entry.path);
        catalog_entry.top_group = TopLevelPathGroup(catalog_entry.path);
        return catalog_entry;
    }

    void MainWindow::UpdateCatalogSnapshot()
    {
        if (!CatalogSnapshotText()) {
            return;
        }

        std::wstring text = L"Catalog entries: " + std::to_wstring(m_tree_catalog.size());
        text += L", visible: " + std::to_wstring(m_instant_search_hits.size());

        if (m_instant_search_hits.empty()) {
            text += L", no matches for current filters";
        } else {
            text += L", top: ";
            const size_t limit = std::min<size_t>(m_instant_search_hits.size(), 3);
            for (size_t index = 0; index < limit; ++index) {
                if (index > 0) {
                    text += L" | ";
                }
                text += m_instant_search_hits[index].name + L" (" +
                    m_instant_search_hits[index].kind + L", " +
                    m_instant_search_hits[index].size_text + L")";
            }
        }
        text += L" | " + m_last_scan_duration_text;

        CatalogSnapshotText().Text(winrt::hstring(text));
    }

    void MainWindow::UpdateTreeSnapshotPreview(std::vector<TreeCatalogEntry> const& entries)
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        using namespace Microsoft::UI::Xaml::Media;

        if (!TreeSnapshotPanel()) {
            return;
        }

        if (TreeListView() && m_tree_updates_ready) {
            PopulateTreeList(entries);
        }

        TreeSnapshotPanel().Children().Clear();
        if (TreeSnapshotExtraPanel()) {
            TreeSnapshotExtraPanel().Children().Clear();
        }
        if (TreeSnapshotExpandButton()) {
            TreeSnapshotExpandButton().IsEnabled(false);
            TreeSnapshotExpandButton().Content(box_value(L"Load more rows"));
        }

        if (entries.empty()) {
            auto empty = TextBlock{};
            empty.Text(L"No catalog tree rows available yet.");
            empty.Opacity(0.72);
            TreeSnapshotPanel().Children().Append(empty);
            if (TreeSnapshotExtraPanel()) {
                TreeSnapshotExtraPanel().Visibility(Visibility::Collapsed);
            }
            return;
        }

        const size_t limit = std::min<size_t>(entries.size(), 6);
        const size_t extra_limit = std::min<size_t>(entries.size(), 12);
        double largest_size = 0.0;
        for (size_t index = 0; index < extra_limit; ++index) {
            largest_size = (std::max)(largest_size, static_cast<double>(entries[index].size_bytes));
        }

        auto append_sample = [&](StackPanel const& target, TreeCatalogEntry const& entry) {
            const double entry_size = static_cast<double>(entry.size_bytes);
            const double ratio = largest_size > 0.0 ? std::clamp(entry_size / largest_size, 0.0, 1.0) : 0.0;
            const double bar_width = ratio > 0.0 ? (std::max)(6.0, 220.0 * ratio) : 0.0;
            const double indent_width = (std::min)(44.0, static_cast<double>(entry.path_depth) * 10.0);

            auto row_button = Button{};
            row_button.Padding(Thickness{ 12.0, 10.0, 12.0, 10.0 });
            row_button.HorizontalAlignment(HorizontalAlignment::Stretch);
            row_button.Background(MakeBrush(ActiveShellTheme().subtle_background));
            row_button.BorderBrush(MakeBrush(ActiveShellTheme().subtle_border));
            row_button.BorderThickness(Thickness{ 1.0, 1.0, 1.0, 1.0 });
            row_button.Tag(box_value(winrt::hstring(
                entry.path + L"|" + entry.kind + L"|" + entry.size_text + L"|" + entry.description)));
            row_button.Click({ this, &MainWindow::OnSearchResultClicked });

            auto row = StackPanel{};
            row.Orientation(Orientation::Vertical);
            row.Spacing(6);

            auto title_row = StackPanel{};
            title_row.Orientation(Orientation::Horizontal);
            title_row.Spacing(8);

            auto indent = Border{};
            indent.Width(indent_width);
            indent.Height(1.0);
            title_row.Children().Append(indent);

            auto row_title = TextBlock{};
            row_title.Text(winrt::hstring(entry.name));
            row_title.Foreground(MakeBrush(ActiveShellTheme().text_primary));
            row_title.TextWrapping(TextWrapping::WrapWholeWords);
            title_row.Children().Append(row_title);
            row.Children().Append(title_row);

            auto size_row = StackPanel{};
            size_row.Orientation(Orientation::Horizontal);
            size_row.Spacing(10);

            auto track = Border{};
            track.Width(220.0);
            track.Height(7.0);
            track.CornerRadius(CornerRadius{ 3.5, 3.5, 3.5, 3.5 });
            track.Background(MakeBrush(ActiveShellTheme().progress_track));
            track.HorizontalAlignment(HorizontalAlignment::Left);

            auto fill = Border{};
            fill.Width(bar_width);
            fill.Height(7.0);
            fill.CornerRadius(CornerRadius{ 3.5, 3.5, 3.5, 3.5 });
            fill.Background(MakeBrush(ActiveShellTheme().progress_fill));
            fill.HorizontalAlignment(HorizontalAlignment::Left);
            track.Child(fill);
            size_row.Children().Append(track);

            auto size_label = TextBlock{};
            size_label.Text(winrt::hstring(entry.size_text));
            size_label.MinWidth(70.0);
            size_label.VerticalAlignment(VerticalAlignment::Center);
            size_label.Opacity(0.8);
            size_row.Children().Append(size_label);
            row.Children().Append(size_row);

            auto row_meta = TextBlock{};
            row_meta.Text(winrt::hstring(
                entry.path + L"  |  " +
                entry.kind + L"  |  " +
                L"level " + std::to_wstring(entry.path_depth) + L"  |  " +
                (entry.parent_path.empty() ? L"parent (root)" : L"parent " + entry.parent_path) + L"  |  " +
                std::to_wstring(static_cast<int>(ratio * 100.0)) + L"% of visible max"));
            row_meta.Opacity(0.72);
            row_meta.TextWrapping(TextWrapping::WrapWholeWords);
            row.Children().Append(row_meta);

            row_button.Content(row);
            target.Children().Append(row_button);
        };

        for (size_t index = 0; index < limit; ++index) {
            append_sample(TreeSnapshotPanel(), entries[index]);
        }

        if (TreeSnapshotExtraPanel()) {
            for (size_t index = limit; index < extra_limit; ++index) {
                append_sample(TreeSnapshotExtraPanel(), entries[index]);
            }
            if (extra_limit > limit) {
                TreeSnapshotExtraPanel().Visibility(Visibility::Collapsed);
            }
        }

        if (TreeSnapshotExpandButton() && extra_limit > limit) {
            TreeSnapshotExpandButton().IsEnabled(true);
        }
    }

    void MainWindow::UpdateBreadcrumbs()
    {
        if (!RootBreadcrumbText() || !SelectionText() || !SelectionSizeText()) {
            return;
        }
        RootBreadcrumbText().Text(winrt::hstring(m_current_root_path));
        SelectionText().Text(winrt::hstring(
            L"Selection: " + m_current_selection_name + L" (" + m_current_selection_kind + L")"));
        SelectionSizeText().Text(winrt::hstring(L"Size: " + m_current_selection_size));

        if (!m_path_breadcrumb_panel) {
            return;
        }

        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;

        m_path_breadcrumb_panel.Children().Clear();

        auto add_breadcrumb = [&](std::wstring const& label, std::wstring const& path, bool active) {
            auto button = Button{};
            button.Content(box_value(winrt::hstring(label)));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                button,
                winrt::hstring(L"Path breadcrumb " + label));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                button,
                winrt::hstring(path.empty() ? L"Current breadcrumb" : L"Set scan root to " + path));
            if (!path.empty()) {
                button.Tag(box_value(winrt::hstring(path)));
                button.Click({ this, &MainWindow::OnBreadcrumbClicked });
            }
            button.Padding(Thickness{ 10.0, 4.0, 10.0, 4.0 });
            button.CornerRadius(CornerRadius{ 999.0, 999.0, 999.0, 999.0 });
            auto background = Microsoft::UI::Xaml::Media::SolidColorBrush{};
            background.Color(active ? ActiveShellTheme().chip_active_background : ActiveShellTheme().subtle_background);
            button.Background(background);
            auto foreground = Microsoft::UI::Xaml::Media::SolidColorBrush{};
            foreground.Color(ActiveShellTheme().text_primary);
            button.Foreground(foreground);
            m_path_breadcrumb_panel.Children().Append(button);
        };

        const std::wstring path = m_current_root_path.empty() ? L"C:\\" : m_current_root_path;
        std::wstring accumulated;
        size_t start = 0;
        if (path.size() >= 3 && path[1] == L':' && (path[2] == L'\\' || path[2] == L'/')) {
            accumulated = path.substr(0, 3);
            add_breadcrumb(accumulated, accumulated, path == accumulated);
            start = 3;
        }

        while (start < path.size()) {
            const auto next = path.find_first_of(L"\\/", start);
            const auto part = path.substr(start, next == std::wstring::npos ? std::wstring::npos : next - start);
            if (!part.empty()) {
                if (!accumulated.empty() && accumulated.back() != L'\\') {
                    accumulated += L'\\';
                }
                accumulated += part;
                add_breadcrumb(part, accumulated, accumulated == path);
            }
            if (next == std::wstring::npos) {
                break;
            }
            start = next + 1;
        }
    }

    void MainWindow::UpdateStatus(std::wstring const& text)
    {
        if (!StatusText()) {
            return;
        }
        StatusText().Text(winrt::hstring(text));
        UpdateSummaryText();
    }

    void MainWindow::UpdateEventText(std::wstring const& text)
    {
        if (!EventText()) {
            return;
        }
        EventText().Text(winrt::hstring(text));
        UpdateSummaryText();
    }

    void MainWindow::UpdateSearchPreview(std::wstring const& text)
    {
        if (!SearchPreviewText()) {
            return;
        }
        SearchPreviewText().Text(winrt::hstring(text));
    }

    void MainWindow::UpdateSearchResultsPreview(std::vector<TreeCatalogEntry> const& hits)
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        using namespace Microsoft::UI::Xaml::Media;

        if (!SearchResultsPanel()) {
            return;
        }

        SearchResultsPanel().Children().Clear();

        if (hits.empty()) {
            auto empty = TextBlock{};
            empty.Text(L"No matches yet.");
            empty.Opacity(0.72);
            SearchResultsPanel().Children().Append(empty);
            return;
        }

        const size_t limit = std::min<size_t>(hits.size(), 5);
        for (size_t index = 0; index < limit; ++index) {
            auto const& entry = hits[index];
            auto row_button = Button{};
            row_button.HorizontalAlignment(HorizontalAlignment::Stretch);
            row_button.Padding(Thickness{ 12.0, 10.0, 12.0, 10.0 });
            row_button.Background(MakeBrush(ActiveShellTheme().subtle_background));
            row_button.BorderBrush(MakeBrush(ActiveShellTheme().subtle_border));
            row_button.BorderThickness(Thickness{ 1.0, 1.0, 1.0, 1.0 });
            row_button.Tag(box_value(winrt::hstring(
                entry.path + L"|" + entry.kind + L"|" + entry.size_text + L"|" + entry.description)));
            row_button.Click({ this, &MainWindow::OnSearchResultClicked });

            auto row = StackPanel{};
            row.Orientation(Orientation::Vertical);
            row.Spacing(2);

            auto row_title = TextBlock{};
            row_title.Text(winrt::hstring(entry.name));
            row_title.Foreground(MakeBrush(ActiveShellTheme().text_primary));
            row.Children().Append(row_title);

            auto row_meta = TextBlock{};
            row_meta.Text(winrt::hstring(entry.path + L"  |  " + entry.kind + L"  |  " + entry.size_text));
            row_meta.Opacity(0.72);
            row_meta.TextWrapping(TextWrapping::WrapWholeWords);
            row.Children().Append(row_meta);



            row_button.Content(row);
            SearchResultsPanel().Children().Append(row_button);
        }
    }

    void MainWindow::RefreshInstantSearch()
    {
        TraceStartup(L"RefreshInstantSearch begin");
        m_instant_search_hits = FilterTreeCatalog();
        auto const& hits = m_instant_search_hits;
        TraceStartup(L"RefreshInstantSearch after filter");
        if (!hits.empty()) {
            auto const& first = hits.front();
            SelectVisualizationTarget(first.name, first.path, first.kind, first.size_text);
        }
        TraceStartup(L"RefreshInstantSearch after selection");

        std::wstring preview = FormatSearchQuery();
        preview += L", results=" + std::to_wstring(hits.size());
        if (hits.empty()) {
            preview += L", no matches";
        } else {
            preview += L", top=\"" + hits.front().name + L"\"";
        }
        UpdateSearchPreview(preview);
        TraceStartup(L"RefreshInstantSearch after search preview");
        UpdateSearchResultsPreview(hits);
        TraceStartup(L"RefreshInstantSearch after search results");
        UpdateEventText(L"Instant search updated.");
        TraceStartup(L"RefreshInstantSearch after event text");
        UpdateTreeSnapshotPreview(hits);
        TraceStartup(L"RefreshInstantSearch after tree preview");
        TraceStartup(L"RefreshInstantSearch end");
    }

    void MainWindow::LoadPersistedCatalogSnapshot()
    {
        TraceStartup(L"LoadPersistedCatalogSnapshot begin");
        if (!m_shell_ready) {
            TraceStartup(L"LoadPersistedCatalogSnapshot skipped: shell not ready");
            return;
        }

        ::WinBlaze::UI::NativeBridge::Initialize();

        m_tree_catalog.clear();
        m_tree_catalog_keys.clear();
        m_instant_search_hits.clear();
        m_tree_window_offset = 0;

        std::vector<TreeCatalogEntry> snapshot;
        const auto stats = ::WinBlaze::UI::NativeBridge::LoadCatalogSnapshotWithStats([&](WbCatalogEntry const& entry) {
            if (snapshot.size() < kCatalogSnapshotLoadLimit) {
                snapshot.push_back(CatalogEntryFromNative(entry));
            }
        });

        m_last_cache_load_text =
            L"Cache load: read " + FormatBytes(stats.cache_read_bytes) +
            L" in " + std::to_wstring(stats.cache_read_millis) + L" ms" +
            L", decoded in " + std::to_wstring(stats.cache_decode_millis) + L" ms" +
            L", entries=" + std::to_wstring(stats.volumes + stats.directories + stats.files) +
            L", load cap=" + std::to_wstring(stats.entries_emitted_limit) +
            L" (volumes=" + std::to_wstring(stats.volumes) +
            L", directories=" + std::to_wstring(stats.directories) +
            L", files=" + std::to_wstring(stats.files) + L")" +
            (stats.cache_loaded_from_backup != 0 ? L", source=backup" : L", source=primary");

        if (snapshot.empty()) {
            TraceStartup(L"LoadPersistedCatalogSnapshot empty");
            UpdateTreeSnapshotPreview(std::vector<TreeCatalogEntry>{});
            UpdateSearchResultsPreview(std::vector<TreeCatalogEntry>{});
            UpdateCatalogSnapshot();
            m_treemap_render_dirty = true;
            ScheduleTreemapRender(L"empty snapshot");
            UpdateSummaryText();
            UpdateRuntimeSnapshot();
            return;
        }
        for (auto const& entry : snapshot) {
            if (m_tree_catalog_keys.insert(TreeCatalogKey(entry)).second) {
                m_tree_catalog.push_back(entry);
            }
        }

        m_has_results = true;
        m_instant_search_hits = FilterTreeCatalog();
        UpdateTreeSnapshotPreview(m_instant_search_hits);
        UpdateSearchResultsPreview(m_instant_search_hits);
        UpdateCatalogSnapshot();
        m_treemap_render_dirty = true;
        ScheduleTreemapRender(L"snapshot loaded");
        UpdateSummaryText();
        UpdateRuntimeSnapshot();
        TraceStartup(L"LoadPersistedCatalogSnapshot end");
    }

    void MainWindow::UpdateProgress(double percent, std::wstring const& text)
    {
        if (!ProgressText()) {
            return;
        }
        if (ScanProgressFill()) {
            const double clamped_percent = std::clamp(percent, 0.0, 100.0);
            ScanProgressFill().Width(3.6 * clamped_percent);
        }
        ProgressText().Text(winrt::hstring(text));
        UpdateSummaryText();
    }

    void MainWindow::HandleNativeEvent(WbEvent const& event)
    {
        std::wstring status_text;
        std::wstring event_text;
        std::wstring summary_text;
        std::wstring progress_text;
        std::wstring error_text;
        std::wstring last_issue_text;
        double progress_percent = 0.0;
        bool clear_session = false;
        bool mark_has_results = false;
        bool reload_snapshot = false;

        switch (event.kind) {
        case WbEventKind_SessionStarted:
            TraceStartup(L"HandleNativeEvent session started");
            status_text = L"Scanning...";
            event_text = L"Session started.";
            progress_text = L"0% complete";
            progress_percent = 0.0;
            m_scan_started_at = std::chrono::steady_clock::now();
            m_last_scan_duration_text = L"Scan duration: in progress";
            m_last_ui_latency_ms = 0.0;
            m_last_input_latency_ms = 0.0;
            m_last_ui_flush_duration_ms = 0.0;
            m_peak_ui_flush_duration_ms = 0.0;
            m_last_composition_frame_ms = 0.0;
            m_peak_composition_frame_ms = 0.0;
            m_last_composition_frame_time = std::chrono::steady_clock::now();
            m_total_ui_flush_count = 0;
            m_total_composition_frame_count = 0;
            m_pending_event_count = 0;
            mark_has_results = true;
            break;
        case WbEventKind_Progress:
            TraceStartup(
                L"HandleNativeEvent progress " +
                std::to_wstring(event.progress_items_done) + L"/" +
                std::to_wstring(event.progress_items_total));
            status_text = L"Scanning...";
            if (event.progress_items_total > 0) {
                progress_percent = (static_cast<double>(event.progress_items_done) / static_cast<double>(event.progress_items_total)) * 100.0;
            }
            if (event.progress_items_total == 0) {
                event_text = L"Scanning in progress: " +
                    std::to_wstring(event.progress_items_done) + L" items discovered";
                progress_text = L"Items discovered: " +
                    std::to_wstring(event.progress_items_done);
            } else {
                event_text = L"Scanning in progress: " +
                    std::to_wstring(event.progress_items_done) + L"/" +
                    std::to_wstring(event.progress_items_total) + L" items";
                progress_text = std::to_wstring(static_cast<int>(progress_percent)) + L"% complete";
            }
            mark_has_results = true;
            break;
        case WbEventKind_Summary:
            TraceStartup(L"HandleNativeEvent summary");
            status_text = L"Finalizing...";
            event_text = FormatSummary(event);
            summary_text = event_text;
            progress_percent = 100.0;
            progress_text = L"100% complete";
            if (m_scan_started_at.time_since_epoch().count() > 0) {
                const auto elapsed = std::chrono::steady_clock::now() - m_scan_started_at;
                m_last_scan_duration_text = L"Scan duration: " +
                    std::to_wstring(static_cast<int>(std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count())) +
                    L" ms";
            }
            mark_has_results = true;
            break;
        case WbEventKind_Completed:
            TraceStartup(L"HandleNativeEvent completed");
            status_text = L"Completed.";
            event_text = L"Scan completed.";
            progress_percent = 100.0;
            progress_text = L"100% complete";
            if (m_scan_started_at.time_since_epoch().count() > 0) {
                const auto elapsed = std::chrono::steady_clock::now() - m_scan_started_at;
                m_last_scan_duration_text = L"Scan duration: " +
                    std::to_wstring(static_cast<int>(std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count())) +
                    L" ms";
            }
            clear_session = true;
            reload_snapshot = true;
            mark_has_results = true;
            break;
        case WbEventKind_Cancelled:
            TraceStartup(L"HandleNativeEvent cancelled");
            status_text = L"Cancelled.";
            event_text = L"Scan cancelled.";
            clear_session = true;
            reload_snapshot = true;
            break;
        case WbEventKind_Failed:
            TraceStartup(L"HandleNativeEvent failed");
            status_text = L"Failed.";
            event_text = L"Scan failed.";
            error_text = L"The scan encountered a recoverable failure and stopped.";
            if (event.error.message.ptr != nullptr && event.error.message.len > 0) {
                error_text = Utf8ToWide(std::string_view{
                    event.error.message.ptr,
                    event.error.message.len,
                });
            }
            ReportFailure(L"scan.failed", error_text);
            clear_session = true;
            m_has_error = true;
            reload_snapshot = true;
            break;
        case WbEventKind_Issue:
        {
            const std::wstring issue_message = Utf8ToWide(std::string_view{
                event.error.message.ptr,
                event.error.message.len,
            });
            const std::wstring issue_label = IssueCodeLabel(static_cast<uint32_t>(event.error.code));
            std::wstring issue_text = L"HandleNativeEvent issue ";
            issue_text += L"code=";
            issue_text += std::to_wstring(static_cast<unsigned int>(event.error.code));
            issue_text += L" message=";
            issue_text += issue_message;
            TraceStartup(issue_text);
            status_text = L"Scanning with issues.";
            event_text = L"A recoverable scan issue was reported (" + issue_label + L" code " +
                std::to_wstring(static_cast<unsigned int>(event.error.code)) + L"): " + issue_message;
            last_issue_text = event_text;
            mark_has_results = true;
            break;
        }
        case WbEventKind_IncrementalChanges:
            TraceStartup(L"HandleNativeEvent incremental changes");
            status_text = L"Incremental rescan changes applied.";
            event_text = L"Incremental changes: added=" +
                std::to_wstring(event.incremental_changes.added) +
                L", removed=" + std::to_wstring(event.incremental_changes.removed) +
                L", modified=" + std::to_wstring(event.incremental_changes.modified) +
                L", renamed=" + std::to_wstring(event.incremental_changes.renamed) +
                L", moved=" + std::to_wstring(event.incremental_changes.moved);
            mark_has_results = true;
            break;
        case WbEventKind_VolumeDiscovered:
            TraceStartup(L"HandleNativeEvent volume discovered");
            status_text = L"Scanning...";
            event_text = L"Volume discovered: " + Utf8ToWide(std::string_view{
                event.catalog_entry.path.ptr,
                event.catalog_entry.path.len,
            });
            mark_has_results = true;
            break;
        case WbEventKind_DirectoryFound:
            TraceStartup(L"HandleNativeEvent directory found");
            status_text = L"Scanning...";
            event_text = L"Directory discovered: " + Utf8ToWide(std::string_view{
                event.catalog_entry.path.ptr,
                event.catalog_entry.path.len,
            });
            mark_has_results = true;
            break;
        case WbEventKind_FileFound:
            status_text = L"Scanning...";
            event_text = L"File discovered: " + Utf8ToWide(std::string_view{
                event.catalog_entry.path.ptr,
                event.catalog_entry.path.len,
            });
            mark_has_results = true;
            break;
        }

        {
            std::lock_guard guard(m_pending_ui_mutex);
            ++m_pending_event_count;
            m_last_ui_event_time = std::chrono::steady_clock::now();

            if (mark_has_results) {
                m_has_results = true;
                m_has_error = false;
            }

            if (!status_text.empty()) {
                m_pending_ui_state.status_dirty = true;
                m_pending_ui_state.status_text = std::move(status_text);
            }

            if (!event_text.empty()) {
                m_pending_ui_state.event_dirty = true;
                m_pending_ui_state.event_text = std::move(event_text);
            }

            if (!summary_text.empty()) {
                m_pending_ui_state.summary_dirty = true;
                m_pending_ui_state.summary_text = std::move(summary_text);
            }

            if (!progress_text.empty()) {
                m_pending_ui_state.progress_dirty = true;
                m_pending_ui_state.progress_percent = progress_percent;
                m_pending_ui_state.progress_text = std::move(progress_text);
            }

            if (!error_text.empty()) {
                m_pending_ui_state.error_dirty = true;
                m_pending_ui_state.error_text = std::move(error_text);
            }

            if (event.kind == WbEventKind_SessionStarted) {
                m_pending_ui_state.diagnostics_dirty = true;
                m_pending_ui_state.correctness_dirty = true;
                m_pending_ui_state.reset_scan_issues = true;
                m_pending_ui_state.progress_items_done = 0;
                m_pending_ui_state.progress_items_total = 0;
                m_pending_ui_state.progress_bytes_done = 0;
                m_pending_ui_state.progress_bytes_total = 0;
                m_pending_ui_state.summary_files_seen = 0;
                m_pending_ui_state.summary_directories_seen = 0;
                m_pending_ui_state.summary_total_size_bytes = 0;
            } else if (event.kind == WbEventKind_Progress) {
                m_pending_ui_state.diagnostics_dirty = true;
                m_pending_ui_state.progress_items_done = event.progress_items_done;
                m_pending_ui_state.progress_items_total = event.progress_items_total;
                m_pending_ui_state.progress_bytes_done = event.progress_bytes_done;
                m_pending_ui_state.progress_bytes_total = event.progress_bytes_total;
            } else if (event.kind == WbEventKind_Summary) {
                m_pending_ui_state.diagnostics_dirty = true;
                m_pending_ui_state.correctness_dirty = true;
                m_pending_ui_state.progress_bytes_done = event.summary.total_size_bytes;
                m_pending_ui_state.progress_bytes_total = event.summary.total_size_bytes;
                m_pending_ui_state.summary_files_seen = event.summary.files_seen;
                m_pending_ui_state.summary_directories_seen = event.summary.directories_seen;
                m_pending_ui_state.summary_total_size_bytes = event.summary.total_size_bytes;
            } else if (event.kind == WbEventKind_Issue) {
                m_pending_ui_state.correctness_dirty = true;
                ++m_pending_ui_state.issue_count_delta;
                ++m_pending_ui_state.issue_code_deltas[static_cast<uint32_t>(event.error.code)];
                m_pending_ui_state.last_issue_text = last_issue_text;
                m_pending_ui_state.recent_issue_texts.push_back(last_issue_text);
            } else if (event.kind == WbEventKind_IncrementalChanges) {
                m_pending_ui_state.correctness_dirty = true;
                m_pending_ui_state.incremental_changes_dirty = true;
                m_pending_ui_state.incremental_added = event.incremental_changes.added;
                m_pending_ui_state.incremental_removed = event.incremental_changes.removed;
                m_pending_ui_state.incremental_modified = event.incremental_changes.modified;
                m_pending_ui_state.incremental_renamed = event.incremental_changes.renamed;
                m_pending_ui_state.incremental_moved = event.incremental_changes.moved;
            }

            if (event.kind == WbEventKind_VolumeDiscovered ||
                event.kind == WbEventKind_DirectoryFound ||
                event.kind == WbEventKind_FileFound ||
                event.kind == WbEventKind_SessionStarted) {
                m_pending_ui_state.catalog_dirty = true;
                if (event.kind != WbEventKind_FileFound || m_pending_ui_state.catalog_entries.size() < 256) {
                    m_pending_ui_state.catalog_entries.push_back(CatalogEntryFromNative(event.catalog_entry));
                }
            }

            m_pending_ui_state.reload_snapshot = reload_snapshot;

            if (clear_session) {
                auto session = m_session;
                m_session = {};
                m_session_active = false;
                std::thread([session]() {
                    ::WinBlaze::UI::NativeBridge::DestroyScan(session);
                }).detach();
            }
        }

        ScheduleUiFlush();
    }

    std::wstring MainWindow::FormatSummary(WbEvent const& event) const
    {
        return L"Summary: " +
            std::to_wstring(event.summary.files_seen) + L" files, " +
            std::to_wstring(event.summary.directories_seen) + L" directories, " +
            std::to_wstring(event.summary.total_size_bytes) + L" bytes";
    }

    std::wstring MainWindow::FormatSearchQuery()
    {
        std::wstring query = L"Search: ";
        const std::wstring pattern = SearchBox() ? SearchBox().Text().c_str() : L"";
        const std::wstring extensions = ExtensionBox() ? ExtensionBox().Text().c_str() : L"";
        const std::wstring minimum_size = MinSizeBox() ? MinSizeBox().Text().c_str() : L"";
        const std::wstring modified_after = ModifiedAfterBox() ? ModifiedAfterBox().Text().c_str() : L"";
        const std::wstring modified_before = ModifiedBeforeBox() ? ModifiedBeforeBox().Text().c_str() : L"";

        if (!pattern.empty()) {
            query += L"pattern=\"" + pattern + L"\"";
        } else {
            query += L"(empty pattern)";
        }

        if (!extensions.empty()) {
            query += L", extensions=\"" + extensions + L"\"";
        }

        if (!minimum_size.empty()) {
            query += L", min-size=\"" + minimum_size + L"\"";
        }

        if (!modified_after.empty()) {
            query += L", modified-after=\"" + modified_after + L"\"";
        }

        if (!modified_before.empty()) {
            query += L", modified-before=\"" + modified_before + L"\"";
        }

        query += L", match=" + ComboBoxSelectionText(MatchModeBox(), L"Substring");
        query += L", sort=" + ComboBoxSelectionText(SortFieldBox(), L"Name");
        query += L", direction=" + ComboBoxSelectionText(SortDirectionBox(), L"Descending");

        return query;
    }

    Microsoft::UI::Xaml::Controls::ListViewItem MainWindow::CreateTreeListItem(TreeCatalogEntry const& entry) const
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        using namespace Microsoft::UI::Xaml::Media;

        auto item = Microsoft::UI::Xaml::Controls::ListViewItem{};
        item.Tag(box_value(winrt::hstring(entry.path + L"|" + entry.kind + L"|" + entry.size_text + L"|" + entry.description)));
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            item,
            winrt::hstring(entry.name + L", " + entry.kind + L", " + entry.size_text));
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            item,
            winrt::hstring(
                entry.kind + L" at " + entry.path +
                L", size " + entry.size_text +
                L", level " + std::to_wstring(entry.path_depth) +
                (entry.parent_path.empty() ? L"" : L", parent " + entry.parent_path) +
                (entry.top_group.empty() ? L"" : L", group " + entry.top_group)));
        auto row = Microsoft::UI::Xaml::Controls::StackPanel{};
        row.Orientation(Microsoft::UI::Xaml::Controls::Orientation::Horizontal);
        row.Spacing(12);
        const double indent_width = (std::min)(56.0, static_cast<double>(entry.path_depth) * 12.0);
        auto label = Microsoft::UI::Xaml::Controls::TextBlock{};
        label.Text(winrt::hstring(entry.name));
        label.Width(170.0);
        label.Margin(Thickness{ indent_width, 0.0, 0.0, 0.0 });
        label.TextWrapping(TextWrapping::WrapWholeWords);
        row.Children().Append(label);

        auto track = Border{};
        track.Width(220.0);
        track.Height(8.0);
        track.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
        track.Background(MakeBrush(ActiveShellTheme().progress_track));
        track.VerticalAlignment(VerticalAlignment::Center);

        auto fill = Border{};
        fill.Width(2.2 * std::clamp(entry.progress, 0, 100));
        fill.Height(8.0);
        fill.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
        fill.Background(MakeBrush(ActiveShellTheme().progress_fill));
        fill.HorizontalAlignment(HorizontalAlignment::Left);
        track.Child(fill);
        row.Children().Append(track);

        auto size_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        size_text.Text(winrt::hstring(entry.size_text));
        size_text.MinWidth(72.0);
        size_text.Opacity(0.8);
        size_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(size_text);

        auto kind_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        kind_text.Text(winrt::hstring(entry.kind));
        kind_text.Opacity(0.72);
        kind_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(kind_text);

        auto level_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        level_text.Text(winrt::hstring(L"Level " + std::to_wstring(entry.path_depth)));
        level_text.MinWidth(64.0);
        level_text.Opacity(0.72);
        level_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(level_text);

        auto parent_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        parent_text.Text(winrt::hstring(entry.parent_path.empty() ? L"(root)" : entry.parent_path));
        parent_text.Width(180.0);
        parent_text.Opacity(0.68);
        parent_text.TextTrimming(TextTrimming::CharacterEllipsis);
        parent_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(parent_text);

        auto path_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        path_text.Text(winrt::hstring(entry.path));
        path_text.Width(360.0);
        path_text.Opacity(0.68);
        path_text.TextTrimming(TextTrimming::CharacterEllipsis);
        path_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(path_text);

        item.Content(row);
        return item;
    }

    void MainWindow::PopulateTreeList(std::vector<TreeCatalogEntry> const& entries)
    {
        if (!TreeListView() || !m_tree_updates_ready) {
            return;
        }

        if (entries.empty()) {
            m_tree_window_offset = 0;
        } else if (m_tree_window_offset >= entries.size()) {
            m_tree_window_offset = ((entries.size() - 1) / kTreeListVirtualizedWindowLimit) * kTreeListVirtualizedWindowLimit;
        }

        const auto window_start = (std::min)(m_tree_window_offset, entries.size());
        const auto window_end = (std::min)(window_start + kTreeListVirtualizedWindowLimit, entries.size());

        m_tree_selection_updates_suppressed = true;
        TreeListView().Items().Clear();
        for (size_t index = window_start; index < window_end; ++index) {
            TreeListView().Items().Append(CreateTreeListItem(entries[index]));
        }
        m_tree_selection_updates_suppressed = false;

        const auto appended = window_end - window_start;
        if (TreeWindowPreviousButton()) {
            TreeWindowPreviousButton().IsEnabled(window_start > 0);
        }
        if (TreeWindowNextButton()) {
            TreeWindowNextButton().IsEnabled(window_end < entries.size());
        }

        if (TreeListStatusText()) {
            std::unordered_set<std::wstring> all_groups;
            std::unordered_set<std::wstring> window_groups;
            for (auto const& entry : entries) {
                if (!entry.top_group.empty()) {
                    all_groups.insert(entry.top_group);
                }
            }
            for (size_t index = window_start; index < window_end; ++index) {
                if (!entries[index].top_group.empty()) {
                    window_groups.insert(entries[index].top_group);
                }
            }

            std::wstring status;
            if (entries.empty()) {
                status = L"Showing 0 of 0 matching catalog rows with virtualized ListView containers.";
            } else {
                status = L"Showing rows " + std::to_wstring(window_start + 1) +
                    L"-" + std::to_wstring(window_end) +
                    L" of " + std::to_wstring(entries.size()) +
                    L" matching catalog rows with virtualized ListView containers, path-depth indentation, and " +
                    std::to_wstring(all_groups.size()) + L" top-level groups";
                if (all_groups.size() != window_groups.size()) {
                    status += L" (" + std::to_wstring(window_groups.size()) + L" in this window)";
                }
            }
            if (entries.size() > kTreeListVirtualizedWindowLimit) {
                status += L"; page size is " +
                    std::to_wstring(kTreeListVirtualizedWindowLimit) +
                    L" rows to keep redraws responsive.";
            } else if (!entries.empty()) {
                status += L".";
            }
            TreeListStatusText().Text(winrt::hstring(status));
        }
    }

    std::vector<MainWindow::TreeCatalogEntry> MainWindow::FilterTreeCatalog() const
    {
        const std::wstring pattern = SearchBox() ? LowercaseCopy(SearchBox().Text().c_str()) : L"";
        const std::wstring extensions = ExtensionBox() ? LowercaseCopy(ExtensionBox().Text().c_str()) : L"";
        const std::wstring minimum_size_text = MinSizeBox() ? MinSizeBox().Text().c_str() : L"";
        const std::wstring modified_after_text = ModifiedAfterBox() ? ModifiedAfterBox().Text().c_str() : L"";
        const std::wstring modified_before_text = ModifiedBeforeBox() ? ModifiedBeforeBox().Text().c_str() : L"";
        const std::wstring match_mode = MatchModeBox() ? ComboBoxSelectionText(MatchModeBox(), L"Substring") : L"Substring";
        const std::wstring sort_field = SortFieldBox() ? ComboBoxSelectionText(SortFieldBox(), L"Name") : L"Name";
        const std::wstring sort_direction = SortDirectionBox() ? ComboBoxSelectionText(SortDirectionBox(), L"Descending") : L"Descending";

        const auto minimum_size = ParseSizeTextBytes(minimum_size_text);

        const auto modified_after = ParseUtcDateBoundary(modified_after_text);
        const auto modified_before = ParseUtcDateBoundary(modified_before_text);
        std::vector<std::wstring> extension_tokens;
        size_t extension_start = 0;
        while (extension_start <= extensions.size()) {
            const auto next = extensions.find(L';', extension_start);
            std::wstring token = extensions.substr(
                extension_start,
                next == std::wstring::npos ? std::wstring::npos : next - extension_start);
            token.erase(std::remove_if(token.begin(), token.end(), ::iswspace), token.end());
            if (!token.empty()) {
                extension_tokens.push_back(std::move(token));
            }
            if (next == std::wstring::npos) {
                break;
            }
            extension_start = next + 1;
        }

        auto matches_text = [&](std::wstring const& value) {
            if (pattern.empty()) {
                return true;
            }

            if (match_mode == L"Exact") {
                return value == pattern ||
                    value.rfind(pattern + L"\n", 0) == 0 ||
                    value.find(L"\n" + pattern + L"\n") != std::wstring::npos ||
                    (value.size() > pattern.size() &&
                        value.compare(value.size() - pattern.size(), pattern.size(), pattern) == 0 &&
                        value[value.size() - pattern.size() - 1] == L'\n');
            }
            if (match_mode == L"Prefix") {
                return value.rfind(pattern, 0) == 0 ||
                    value.find(L"\n" + pattern) != std::wstring::npos;
            }
            return value.find(pattern) != std::wstring::npos;
        };

        auto matches_extension = [&](TreeCatalogEntry const& entry) {
            if (extension_tokens.empty()) {
                return true;
            }

            if (entry.extension_lower.empty()) {
                return false;
            }
            for (auto const& token : extension_tokens) {
                if (token == entry.extension_lower) {
                    return true;
                }
            }

            return false;
        };

        auto matches_size = [&](TreeCatalogEntry const& entry) {
            if (!minimum_size) {
                return true;
            }

            return entry.size_bytes >= minimum_size.value();
        };

        auto matches_modified = [&](TreeCatalogEntry const& entry) {
            if (!modified_after && !modified_before) {
                return true;
            }

            if (!entry.modified_utc) {
                return false;
            }

            if (modified_after && entry.modified_utc.value() < modified_after.value()) {
                return false;
            }
            if (modified_before && entry.modified_utc.value() >= modified_before.value()) {
                return false;
            }
            return true;
        };

        std::vector<TreeCatalogEntry> entries;
        entries.reserve(m_tree_catalog.size());
        for (auto const& entry : m_tree_catalog) {
            if (matches_text(entry.search_text_lower)) {
                if (matches_extension(entry) && matches_size(entry) && matches_modified(entry)) {
                    entries.push_back(entry);
                }
            }
        }

        std::sort(entries.begin(), entries.end(), [&](TreeCatalogEntry const& left, TreeCatalogEntry const& right) {
            int comparison = 0;
            if (sort_field == L"Size") {
                const auto left_size = left.size_bytes;
                const auto right_size = right.size_bytes;
                comparison = left_size < right_size ? -1 : (left_size > right_size ? 1 : 0);
            } else if (sort_field == L"Type") {
                comparison = _wcsicmp(left.kind.c_str(), right.kind.c_str());
            } else {
                comparison = _wcsicmp(left.name.c_str(), right.name.c_str());
            }

            if (comparison == 0) {
                comparison = _wcsicmp(left.path.c_str(), right.path.c_str());
            }

            return sort_direction == L"Ascending" ? comparison < 0 : comparison > 0;
        });

        return entries;
    }

    bool MainWindow::MatchesInstantSearch(TreeCatalogEntry const& entry) const
    {
        const std::wstring pattern = SearchBox() ? LowercaseCopy(SearchBox().Text().c_str()) : L"";
        const std::wstring extensions = ExtensionBox() ? LowercaseCopy(ExtensionBox().Text().c_str()) : L"";
        const std::wstring minimum_size_text = MinSizeBox() ? MinSizeBox().Text().c_str() : L"";
        const std::wstring modified_after_text = ModifiedAfterBox() ? ModifiedAfterBox().Text().c_str() : L"";
        const std::wstring modified_before_text = ModifiedBeforeBox() ? ModifiedBeforeBox().Text().c_str() : L"";
        const std::wstring match_mode = MatchModeBox() ? ComboBoxSelectionText(MatchModeBox(), L"Substring") : L"Substring";

        const auto minimum_size = ParseSizeTextBytes(minimum_size_text);

        const auto modified_after = ParseUtcDateBoundary(modified_after_text);
        const auto modified_before = ParseUtcDateBoundary(modified_before_text);
        std::vector<std::wstring> extension_tokens;
        size_t extension_start = 0;
        while (extension_start <= extensions.size()) {
            const auto next = extensions.find(L';', extension_start);
            std::wstring token = extensions.substr(
                extension_start,
                next == std::wstring::npos ? std::wstring::npos : next - extension_start);
            token.erase(std::remove_if(token.begin(), token.end(), ::iswspace), token.end());
            if (!token.empty()) {
                extension_tokens.push_back(std::move(token));
            }
            if (next == std::wstring::npos) {
                break;
            }
            extension_start = next + 1;
        }

        auto matches_text = [&](std::wstring const& value) {
            if (pattern.empty()) {
                return true;
            }

            if (match_mode == L"Exact") {
                return value == pattern ||
                    value.rfind(pattern + L"\n", 0) == 0 ||
                    value.find(L"\n" + pattern + L"\n") != std::wstring::npos ||
                    (value.size() > pattern.size() &&
                        value.compare(value.size() - pattern.size(), pattern.size(), pattern) == 0 &&
                        value[value.size() - pattern.size() - 1] == L'\n');
            }
            if (match_mode == L"Prefix") {
                return value.rfind(pattern, 0) == 0 ||
                    value.find(L"\n" + pattern) != std::wstring::npos;
            }
            return value.find(pattern) != std::wstring::npos;
        };

        auto matches_extension = [&]() {
            if (extension_tokens.empty()) {
                return true;
            }

            if (entry.extension_lower.empty()) {
                return false;
            }
            for (auto const& token : extension_tokens) {
                if (token == entry.extension_lower) {
                    return true;
                }
            }

            return false;
        };

        auto matches_size = [&]() {
            if (!minimum_size) {
                return true;
            }

            return entry.size_bytes >= minimum_size.value();
        };

        auto matches_modified = [&]() {
            if (!modified_after && !modified_before) {
                return true;
            }

            if (!entry.modified_utc) {
                return false;
            }

            if (modified_after && entry.modified_utc.value() < modified_after.value()) {
                return false;
            }
            if (modified_before && entry.modified_utc.value() >= modified_before.value()) {
                return false;
            }
            return true;
        };

        return matches_text(entry.search_text_lower)
            && matches_extension()
            && matches_size()
            && matches_modified();
    }

    std::wstring MainWindow::TreeCatalogKey(TreeCatalogEntry const& entry) const
    {
        return entry.name + L"\u001f" + entry.path + L"\u001f" + entry.kind;
    }

    std::wstring MainWindow::SectionName(ShellSection section) const
    {
        switch (section) {
        case ShellSection::Overview:
            return L"Overview";
        case ShellSection::Tree:
            return L"Tree";
        case ShellSection::Treemap:
            return L"Treemap";
        case ShellSection::Search:
            return L"Search";
        case ShellSection::Diagnostics:
            return L"Diagnostics";
        }

        return L"Overview";
    }

    std::wstring MainWindow::ComboBoxSelectionText(
        Microsoft::UI::Xaml::Controls::ComboBox const& box,
        wchar_t const* fallback) const
    {
        if (!box) {
            return fallback;
        }
        if (auto item = box.SelectedItem().try_as<Microsoft::UI::Xaml::Controls::ComboBoxItem>()) {
            return winrt::unbox_value_or<winrt::hstring>(item.Content(), winrt::hstring(fallback)).c_str();
        }

        return fallback;
    }

    std::wstring MainWindow::CurrentVisualizationLabel() const
    {
        return m_current_selection_name + L" - " + m_current_selection_kind;
    }

    void MainWindow::SelectVisualizationTarget(
        std::wstring const& name,
        std::wstring const& path,
        std::wstring const& kind,
        std::wstring const& size_text)
    {
        m_current_selection_name = name;
        m_current_selection_path = path;
        m_current_selection_kind = kind;
        m_current_selection_size = size_text;
        UpdateBreadcrumbs();
        UpdateEventText(L"Selected " + CurrentVisualizationLabel() + L" at " + m_current_selection_path);
        UpdateStatus(L"Selection updated.");
        UpdateSummaryText();

        if (!VolumeDetailPanel() || !FolderDetailPanel() || !FileDetailPanel() || !TreemapZoomOverlay()) {
            return;
        }
        ApplyTreemapColorRules(kind, TreemapZoomOverlay());
    }

    void MainWindow::UpdateTreemapFocus(
        std::wstring const& name,
        std::wstring const& path,
        std::wstring const& kind,
        std::wstring const& size_text)
    {
        if (!TreemapZoomTitle() || !TreemapZoomDescription() || !TreemapZoomOverlay()) {
            return;
        }
        TreemapZoomTitle().Text(winrt::hstring(name + L" - " + kind));
        TreemapZoomDescription().Text(winrt::hstring(path + L" | " + size_text));

        auto const& theme = ActiveShellTheme();
        auto brush = Microsoft::UI::Xaml::Media::SolidColorBrush{};
        if (kind == L"Volume") {
            brush.Color(theme.volume_accent);
        } else if (kind == L"Folder" || kind == L"Directory") {
            brush.Color(theme.folder_accent);
        } else {
            brush.Color(theme.file_accent);
        }
        TreemapZoomOverlay().Background(brush);
        TreemapZoomOverlay().Visibility(Microsoft::UI::Xaml::Visibility::Visible);
    }

    void MainWindow::ApplyTreemapColorRules(
        std::wstring const& kind,
        Microsoft::UI::Xaml::Controls::Border const& panel)
    {
        if (!panel) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto brush = Microsoft::UI::Xaml::Media::SolidColorBrush{};
        if (kind == L"Volume") {
            brush.Color(theme.volume_accent);
        } else if (kind == L"Folder" || kind == L"Directory") {
            brush.Color(theme.folder_accent);
        } else {
            brush.Color(theme.file_accent);
        }
        panel.Background(brush);
    }

    void MainWindow::SetControlVisibility(Microsoft::UI::Xaml::FrameworkElement const& control, bool visible)
    {
        if (!control) {
            return;
        }
        control.Visibility(visible ? Microsoft::UI::Xaml::Visibility::Visible : Microsoft::UI::Xaml::Visibility::Collapsed);
    }

    void MainWindow::FocusSearchBox()
    {
        if (!SearchBox()) {
            return;
        }
        SearchBox().Focus(Microsoft::UI::Xaml::FocusState::Programmatic);
    }

    void MainWindow::FocusRootPathBox()
    {
        if (!RootPathBox()) {
            return;
        }
        RootPathBox().Focus(Microsoft::UI::Xaml::FocusState::Programmatic);
    }

    void MainWindow::NavigateToSection(ShellSection section)
    {
        SetSection(section);
    }
}
