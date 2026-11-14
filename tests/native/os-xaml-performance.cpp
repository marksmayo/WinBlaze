// Performance contracts of the OS XAML host that a benchmark cannot pin down:
// what must not run on the UI thread, and what must happen without a timer hop.
#include "full-os-candidate/MainWindow.xaml.h"
#include "full-os-candidate/NativeBridge.h"
#include <chrono>
#include <cmath>
#include <functional>
#include <iostream>
#include <string>
#include <fstream>
#include <winrt/Windows.UI.Xaml.Automation.Peers.h>
#include <winrt/Windows.UI.Xaml.Automation.Provider.h>

using namespace winrt::Windows::UI::Xaml;
using namespace winrt::Windows::UI::Xaml::Controls;
using AppWindow = winrt::WinBlaze::UI::implementation::MainWindow;

static void Require(bool value, char const* message) { if (!value) throw std::runtime_error(message); }

static bool PumpUntil(std::function<bool()> const& done, int timeout_ms)
{
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
    while (!done()) {
        if (std::chrono::steady_clock::now() > deadline) return false;
        MSG message{};
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        Sleep(1);
    }
    return true;
}

static CheckBox FindCheckBox(winrt::Windows::Foundation::IInspectable const& value, std::wstring_view name)
{
    if (!value) return nullptr;
    if (auto box = value.try_as<CheckBox>(); box && Automation::AutomationProperties::GetName(box) == name) return box;
    auto inspect = [&](auto const& child) { return FindCheckBox(child, name); };
    if (auto panel = value.try_as<Panel>()) {
        for (auto child : panel.Children()) if (auto found = inspect(child)) return found;
    } else if (auto border = value.try_as<Border>()) {
        return inspect(border.Child());
    } else if (auto scroll = value.try_as<ScrollViewer>()) {
        return inspect(scroll.Content());
    } else if (auto content = value.try_as<ContentControl>()) {
        return inspect(content.Content());
    }
    return nullptr;
}

static void IdleShellContracts(winrt::com_ptr<AppWindow> const& window)
{
    using std::chrono::milliseconds;
    Require(!::WinBlaze::UI::NativeBridge::WaitForPrestartedImport(milliseconds(5000)),
        "With no saved scan on the command line the host must not wait for an import");
    // A scan starts with its last refresh long ago (a default time point).
    Require(!AppWindow::LiveTreeRefreshDue(milliseconds(150), milliseconds(3600000)),
        "A scan that may finish within moments must not first build and lay out live rows");
    Require(AppWindow::LiveTreeRefreshDue(milliseconds(400), milliseconds(3600000)),
        "A scan still running after 400 ms must show live rows");
    Require(!AppWindow::LiveTreeRefreshDue(milliseconds(5000), milliseconds(1999)),
        "Live rows must not be rebuilt more often than every two seconds");
    Require(AppWindow::LiveTreeRefreshDue(milliseconds(5000), milliseconds(2000)),
        "Live rows must follow a long scan");
    Require(AppWindow::ScanTreemapRenderDue(milliseconds(3600000)),
        "A scan's first catalog may draw the treemap");
    Require(!AppWindow::ScanTreemapRenderDue(milliseconds(1999)),
        "During a scan the treemap must not redraw on every 100 ms flush");
    Require(AppWindow::ScanTreemapRenderDue(milliseconds(2000)),
        "The treemap must still follow a long scan's catalog");
    auto probe = [&] { return window->PerformanceProbeForTests(); };
    window->OnWindowLoaded(nullptr, nullptr);
    Require(!probe().frame_stats_subscribed,
        "Frame statistics must not tick the UI thread while diagnostics are hidden");
    Require(!GetModuleHandleW(L"winblaze_native.dll"),
        "Loaded must not load the engine or restore the catalog before the first frame");
    Require(PumpUntil([] { return GetModuleHandleW(L"winblaze_native.dll") != nullptr; }, 15000),
        "The deferred catalog restore must still run once the window is idle");
    // The explorer panes sit in a vertical ScrollViewer. Unless the panes are
    // given the viewport's height, they measure at unbounded height: the
    // folder list then lays out every row and the page scrolls instead of it.
    ScrollViewer shell_scroll{ nullptr };
    for (auto child : window->Content().as<Panel>().Children()) {
        if (auto scroll = child.try_as<ScrollViewer>(); scroll && scroll.Content().try_as<Grid>()) shell_scroll = scroll;
    }
    Require(bool(shell_scroll), "The explorer panes must sit in the shell ScrollViewer");
    auto shell = shell_scroll.Content().as<Grid>();
    Require(PumpUntil([&] { return shell_scroll.ViewportHeight() > 0.0 && shell.ActualHeight() > 0.0; }, 10000),
        "The shell must be laid out");
    Require(PumpUntil([&] { return !std::isnan(shell.Height()); }, 2000),
        "The explorer panes must be sized to the viewport, not measured at unbounded height");
    Require(shell.Height() + 0.5 >= shell_scroll.ViewportHeight() - shell.Margin().Top - shell.Margin().Bottom,
        "The explorer panes must fill the viewport when they fit");

    // The first frame creates the render stack from the coalescing timer.
    window->NotifyTreemapSurfaceResized(640, 480);
    Require(PumpUntil([&] { return probe().treemap_renders >= 1; }, 10000),
        "The first treemap frame must render");
    Sleep(60);
    const auto rendered = probe().treemap_renders;
    window->NotifyTreemapSurfaceResized(800, 600);
    Require(probe().treemap_renders == rendered + 1,
        "A resize of an idle treemap must redraw at once, not after a timer");
    Require(probe().swap_chain_creations == 1,
        "A resize must resize the existing swap chain, not create another");
    window->NotifyTreemapSurfaceResized(801, 600);
    Require(probe().treemap_renders == rendered + 1,
        "Resizes arriving faster than a frame must coalesce");
    Require(PumpUntil([&] { return probe().treemap_renders == rendered + 2; }, 5000),
        "A coalesced resize must still be drawn");
    Require(probe().swap_chain_creations == 1, "Coalesced resizes must keep the swap chain");
    Require(probe().treemap_status_writes == 0,
        "Diagnostics text must not be rewritten while its panel has never been shown");

    using Section = winrt::WinBlaze::UI::implementation::ShellSection;
    window->SetSection(Section::Diagnostics);
    Require(probe().frame_stats_subscribed, "Visible diagnostics must collect frame statistics");
    Sleep(60);
    window->NotifyTreemapSurfaceResized(900, 600);
    Require(probe().treemap_status_writes > 0, "Visible diagnostics must show the current treemap state");
    auto toggle = FindCheckBox(window->Content(), L"Runtime metrics");
    Require(bool(toggle), "The runtime metrics toggle must exist once diagnostics are shown");
    // The Diagnostics section keeps its panel open; leave it before hiding.
    window->SetSection(Section::Overview);
    toggle.IsChecked(false);
    window->OnOptionalPanelToggleClicked(nullptr, nullptr);
    Require(!probe().frame_stats_subscribed, "Hiding diagnostics must stop frame statistics");
    std::cout << "PASS: idle shell keeps diagnostics, engine load and treemap timers off the UI thread\n";
}

// The explorer content's scroll viewer, a direct child of the root grid.
static ScrollViewer ShellScroll(winrt::com_ptr<AppWindow> const& window)
{
    if (auto root = window->Content().try_as<Panel>()) {
        for (auto child : root.Children()) if (auto scroll = child.try_as<ScrollViewer>()) return scroll;
    }
    return nullptr;
}

// Run before anything pumps messages: results that load before the first
// layout are laid out at once, and an unbounded first measure of the panes
// took 700 ms on a 250-folder scan.
static void ShellBoundedBeforeLayout(winrt::com_ptr<AppWindow> const& window)
{
    Require(!window->PerformanceProbeForTests().text_inputs_built,
        "The path and scan-rule boxes must not be built before the first frame");
    auto scroll = ShellScroll(window);
    Require(scroll != nullptr, "The explorer content must sit in a scroll viewer");
    Require(scroll.VerticalScrollBarVisibility() == ScrollBarVisibility::Disabled,
        "Before its first layout the explorer content must be bounded to the window");
}

static void ShellScrollContracts(winrt::com_ptr<AppWindow> const& window)
{
    auto scroll = ShellScroll(window);
    HWND owner{};
    winrt::check_hresult(window.as<IWindowNative>()->get_WindowHandle(&owner));
    window->Activate();
    SetWindowPos(owner, nullptr, 40, 40, 900, 360, SWP_NOZORDER | SWP_NOACTIVATE);
    Require(PumpUntil([&] { return scroll.ScrollableHeight() > 0.0; }, 5000),
        "A window too short for the panes' minimum heights must scroll, not clip the treemap");
    Require(PumpUntil([&] { return window->PerformanceProbeForTests().text_inputs_built; }, 5000),
        "The path and scan-rule boxes must be built once the first frame is up");
    Require(!window->PerformanceProbeForTests().root_path_text.empty(),
        "The path box must start with the default scan root");
    std::cout << "PASS: explorer content is bounded before its first layout and scrolls when too short\n";
}

static bool ResultsTitleShown(winrt::com_ptr<AppWindow> const& window)
{
    HWND owner{};
    winrt::check_hresult(window.as<IWindowNative>()->get_WindowHandle(&owner));
    wchar_t title[512]{};
    GetWindowTextW(owner, title, 512);
    const std::wstring_view text{ title };
    return text.rfind(L"WinBlaze - ", 0) == 0 && text.find(L"Scanning") == std::wstring_view::npos;
}

// What the window holds at the moment its title announces results: only the
// work a viewer sees first may stand between the data and that moment.
static void ResultsFirstContracts(winrt::com_ptr<AppWindow> const& window)
{
    const auto probe = window->PerformanceProbeForTests();
    Require(probe.rows_at_results_title >= 1, "The folder rows must be shown with the results");
    Require(probe.rows_at_results_title <= 24,
        "Only about a screenful of folder rows may be built before the results show");
    Require(probe.catalog_entries_at_results_title == 0,
        "The flat search catalog must load after the results show, not before");
    Require(probe.search_hits_at_results_title == 0,
        "The search preview must be filled after the results show, not before");
    Require(probe.treemap_renders_while_results_pending == 0,
        "No treemap may be drawn for data that is about to be replaced by the results");
    Require(PumpUntil([&] { return window->PerformanceProbeForTests().catalog_entries > 0; }, 15000),
        "The flat search catalog must still load once the results show");
    // Replaced results are freed by then; the process heap returns their
    // pages at once instead of up to a second later.
    Require(PumpUntil([&] { return window->PerformanceProbeForTests().freed_heap_returns >= 1; }, 2000),
        "Freed heap pages must be returned once the replaced results are released");
}

static void SavedScanContracts(winrt::com_ptr<AppWindow> const& window)
{
    namespace Bridge = ::WinBlaze::UI::NativeBridge;
    // Shown at once, as the host does, so the first layout comes when it would.
    window->Activate();
    Require(Bridge::ImportPrestartCount() == 1,
        "A saved scan named on the command line must start importing during construction");
    // The host waits this long after the show so the results land in the first frame.
    Require(Bridge::WaitForPrestartedImport(std::chrono::milliseconds(30000)),
        "The prestarted import must report when it has finished");
    Require(PumpUntil([&] { return ResultsTitleShown(window); }, 30000), "The saved scan must finish loading");
    Require(Bridge::ImportRunCount() == 1, "The import begun at launch must be reused, not repeated");
    ResultsFirstContracts(window);
    // A process's first TextBox costs ~16 ms of one-time text-input setup.
    const auto probe = window->PerformanceProbeForTests();
    Require(!probe.text_inputs_at_results_title,
        "The path and scan-rule boxes must be built after the results show, not before");
    Require(PumpUntil([&] { return window->PerformanceProbeForTests().window_loaded; }, 10000),
        "The window must finish its first layout");
    const auto loaded = window->PerformanceProbeForTests();
    Require(!loaded.text_inputs_at_first_layout,
        "The path and scan-rule boxes must not be laid out in the first frame");
    Require(loaded.rows_at_first_layout >= 1 && loaded.rows_at_first_layout <= 24,
        "Folder rows past the first screenful must be added after the first frame");
    Require(PumpUntil([&] { return window->PerformanceProbeForTests().text_inputs_built; }, 5000),
        "The path and scan-rule boxes must be built once the first frame is up");
    Require(!window->PerformanceProbeForTests().root_path_text.empty(),
        "The path box must start with the shown results' root");
    std::cout << "PASS: command-line saved scan imports once, starting before the first frame, results first\n";
}

static Button FindButton(winrt::Windows::Foundation::IInspectable const& value, std::wstring_view name)
{
    if (!value) return nullptr;
    if (auto button = value.try_as<Button>(); button && Automation::AutomationProperties::GetName(button) == name) return button;
    auto inspect = [&](auto const& child) { return FindButton(child, name); };
    if (auto panel = value.try_as<Panel>()) {
        for (auto child : panel.Children()) if (auto found = inspect(child)) return found;
    } else if (auto border = value.try_as<Border>()) {
        return inspect(border.Child());
    } else if (auto scroll = value.try_as<ScrollViewer>()) {
        return inspect(scroll.Content());
    } else if (auto content = value.try_as<ContentControl>()) {
        return inspect(content.Content());
    }
    return nullptr;
}

// One added file, then "Incremental rescan": the engine patches its model
// from recorded changes and the tree updates the rows that changed in
// place, instead of rebuilding every row (a full rebuild of a 250-folder
// root cost ~30 ms of the refresh).
static void InPlaceRefreshContracts(winrt::com_ptr<AppWindow> const& window, std::wstring const& root)
{
    window->Activate();
    const auto probe = [&] { return window->PerformanceProbeForTests(); };
    // The root, 41 folders and one file, once every queued chunk is appended.
    Require(PumpUntil([&] { return probe().tree_rows == 43; }, 10000), "The fixture's 43 tree rows must be shown");
    const auto shown = probe().results_shown;
    const auto added = root + L"\\added.txt";
    { std::ofstream file(added); file << "added"; }
    auto button = FindButton(window->Content(), L"Incremental rescan");
    Require(button != nullptr, "The incremental rescan button must exist");
    Automation::Peers::ButtonAutomationPeer(button).as<Automation::Provider::IInvokeProvider>().Invoke();
    Require(PumpUntil([&] { return probe().results_shown > shown; }, 10000), "The refresh must show results");
    const auto after = probe();
    DeleteFileW(added.c_str());
    Require(after.tree_in_place_refreshes == 1,
        "A refresh the engine patched must update the shown tree in place, not reload it");
    Require(after.tree_rows == 44, "The added file must appear as one more row");
}

static void FolderScanContracts(winrt::com_ptr<AppWindow> const& window, std::wstring const& root)
{
    Require(window->PerformanceProbeForTests().session_active,
        "A folder named on the command line must be scanning when the window has been built");
    Require(PumpUntil([&] { return ResultsTitleShown(window); }, 30000), "The folder scan must finish");
    ResultsFirstContracts(window);
    std::cout << "PASS: command-line folder scan starts during construction, results first\n";
    InPlaceRefreshContracts(window, root);
    std::cout << "PASS: a refresh patched from recorded changes updates the tree in place\n";
}

int wmain(int argc, wchar_t** argv)
{
    int status = 0;
    try {
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        auto manager = Hosting::WindowsXamlManager::InitializeForCurrentThread();
        auto window = winrt::make_self<AppWindow>();
        if (argc == 2) {
            const DWORD attributes = GetFileAttributesW(argv[1]);
            if (attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY)) FolderScanContracts(window, argv[1]);
            else SavedScanContracts(window);
        } else {
            ShellBoundedBeforeLayout(window);
            IdleShellContracts(window);
            ShellScrollContracts(window);
        }
    } catch (std::exception const& error) { std::cerr << "FAIL: " << error.what() << '\n'; status = 1; }
      catch (winrt::hresult_error const& error) { std::wcerr << L"FAIL: " << error.message().c_str() << L'\n'; status = 2; }
    std::cout.flush();
    std::cerr.flush();
    std::wcerr.flush();
    // Like the app itself, end here: background loads may still be running.
    TerminateProcess(GetCurrentProcess(), static_cast<UINT>(status));
    return status;
}
