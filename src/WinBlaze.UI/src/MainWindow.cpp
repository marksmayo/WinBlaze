#include "pch.h"
#include "NativeBridge.h"
#include "MainWindow.xaml.h"
#include "ShellTheme.h"
#include "StartupTrace.h"

#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Web.Http.h>
#include <winrt/Windows.Web.Http.Headers.h>
#include <winrt/Windows.Storage.Streams.h>

#include <filesystem>
#include <fstream>

namespace
{
    constexpr int64_t kFileTimeTicksPerSecond = 10'000'000LL;
    constexpr int64_t kUnixToFileTimeSeconds = 11'644'473'600LL;
    constexpr size_t kTreeListVirtualizedWindowLimit = 256;
    constexpr size_t kCatalogSnapshotLoadLimit = 8192;
    constexpr size_t kRecentIssueLimit = 6;
    // Mirrors winblaze_core::ScanIssueKind::FastScanUnavailable's wire code
    // (see WinBlaze.Native/src/bridge.rs); the fast NTFS-MFT reader was
    // unavailable and the scan fell back to the slower directory walk.
    constexpr uint32_t kFastScanUnavailableIssueCode = 16;

    // In-app update check. Bump kCurrentVersion with each release (matches the
    // Package.appxmanifest identity). TODO: source it from the exe version
    // resource so there is a single source of truth.
    const std::string kCurrentVersion = "0.8.0";
    constexpr wchar_t kReleasesLatestUrl[] =
        L"https://github.com/marksmayo/WinBlaze/releases/latest";
    constexpr wchar_t kReleaseApiUrl[] =
        L"https://api.github.com/repos/marksmayo/WinBlaze/releases/latest";

    // GETs the GitHub releases/latest body off the UI thread; returns an empty
    // string on any network/HTTP failure so callers treat it as "unknown".
    winrt::Windows::Foundation::IAsyncOperation<winrt::hstring> FetchReleaseJsonAsync()
    {
        try {
            winrt::Windows::Web::Http::HttpClient client;
            client.DefaultRequestHeaders().UserAgent().TryParseAdd(L"WinBlaze");
            client.DefaultRequestHeaders().Accept().TryParseAdd(L"application/vnd.github+json");
            winrt::Windows::Foundation::Uri uri{ kReleaseApiUrl };
            auto response = co_await client.GetAsync(uri);
            if (!response.IsSuccessStatusCode()) {
                co_return winrt::hstring{};
            }
            co_return co_await response.Content().ReadAsStringAsync();
        }
        catch (...) {
            co_return winrt::hstring{};
        }
    }

    // Settings "Check for updates": reports the result into the status text and
    // reveals the download button when a newer release exists.
    winrt::fire_and_forget CheckForUpdatesAsync(
        winrt::Microsoft::UI::Xaml::Controls::TextBlock status,
        winrt::Microsoft::UI::Xaml::Controls::Button download)
    {
        using winrt::Microsoft::UI::Xaml::Visibility;
        auto ui_thread = winrt::apartment_context();
        status.Text(L"Checking for updates...");
        download.Visibility(Visibility::Collapsed);

        const winrt::hstring json = co_await FetchReleaseJsonAsync();
        bool ok = false;
        bool available = false;
        std::string latest;
        if (!json.empty()) {
            const std::string body = winrt::to_string(json);
            const WbUpdateCheck check =
                WinBlaze::UI::NativeBridge::CheckForUpdate(kCurrentVersion, body);
            if (check.parsed != 0) {
                ok = true;
                available = check.available != 0;
                latest.assign(reinterpret_cast<const char*>(check.latest), check.latest_len);
            }
        }

        co_await ui_thread;
        const std::wstring current(winrt::to_hstring(kCurrentVersion).c_str());
        if (!ok) {
            status.Text(L"Couldn't check for updates (are you online?).");
        }
        else if (available) {
            const std::wstring latest_w(winrt::to_hstring(latest).c_str());
            status.Text(winrt::hstring(
                L"Update available: " + latest_w + L" - you have " + current + L"."));
            download.Visibility(Visibility::Visible);
        }
        else {
            status.Text(winrt::hstring(L"You're on the latest version (" + current + L")."));
        }
    }

    // Runs a console command with no window and waits (up to 2 min); returns
    // its exit code, or -1 if it couldn't be launched.
    int RunAndWait(std::wstring command_line)
    {
        STARTUPINFOW si{};
        si.cb = sizeof(si);
        PROCESS_INFORMATION pi{};
        std::vector<wchar_t> buffer(command_line.begin(), command_line.end());
        buffer.push_back(L'\0'); // CreateProcessW may write to the buffer.
        if (!CreateProcessW(nullptr, buffer.data(), nullptr, nullptr, FALSE,
                            CREATE_NO_WINDOW, nullptr, nullptr, &si, &pi)) {
            return -1;
        }
        WaitForSingleObject(pi.hProcess, 120'000);
        DWORD code = 1;
        GetExitCodeProcess(pi.hProcess, &code);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        return static_cast<int>(code);
    }

    // Downloads the portable zip for `tag` from the release, extracts it with
    // the bundled tar.exe, then hands off to winblaze-updater.exe (copied to a
    // temp dir so the install-dir copy can be overwritten) which waits for this
    // process to exit, swaps the files, and relaunches. On success the app
    // exits; on any failure it falls back to opening the download page.
    // NOTE: relies on the release asset being named
    // `WinBlaze-<tag>-windows-x64-portable.zip` (the release convention).
    winrt::fire_and_forget InstallUpdateAsync(std::wstring tag)
    {
        namespace WSS = winrt::Windows::Storage::Streams;
        auto ui_thread = winrt::apartment_context();

        wchar_t exe_buf[MAX_PATH]{};
        GetModuleFileNameW(nullptr, exe_buf, MAX_PATH);
        const std::filesystem::path exe_path(exe_buf);
        const std::filesystem::path install_dir = exe_path.parent_path();
        wchar_t temp_buf[MAX_PATH]{};
        GetTempPathW(MAX_PATH, temp_buf);
        const std::filesystem::path work = std::filesystem::path(temp_buf) / L"WinBlazeUpdate";
        const std::filesystem::path stage = work / L"stage";
        const std::filesystem::path zip_path = work / L"download.zip";
        std::error_code ec;
        std::filesystem::remove_all(work, ec);
        std::filesystem::create_directories(stage, ec);

        bool ok = false;
        try {
            const std::wstring url =
                L"https://github.com/marksmayo/WinBlaze/releases/download/" + tag +
                L"/WinBlaze-" + tag + L"-windows-x64-portable.zip";
            winrt::Windows::Web::Http::HttpClient client;
            client.DefaultRequestHeaders().UserAgent().TryParseAdd(L"WinBlaze");
            auto response = co_await client.GetAsync(winrt::Windows::Foundation::Uri{ url });
            if (response.IsSuccessStatusCode()) {
                auto content = co_await response.Content().ReadAsBufferAsync();
                const uint32_t length = content.Length();
                if (length > 0) {
                    auto reader = WSS::DataReader::FromBuffer(content);
                    std::vector<uint8_t> bytes(length);
                    reader.ReadBytes(bytes);
                    std::ofstream out(zip_path, std::ios::binary);
                    out.write(reinterpret_cast<const char*>(bytes.data()),
                              static_cast<std::streamsize>(bytes.size()));
                    out.close();
                    ok = std::filesystem::exists(zip_path);
                }
            }
        }
        catch (...) {
            ok = false;
        }

        if (ok) {
            const std::wstring cmd =
                L"tar.exe -xf \"" + zip_path.wstring() + L"\" -C \"" + stage.wstring() + L"\"";
            ok = RunAndWait(cmd) == 0;
        }

        if (ok) {
            const std::filesystem::path updater_tmp = work / L"winblaze-updater.exe";
            std::filesystem::copy_file(install_dir / L"winblaze-updater.exe", updater_tmp,
                                       std::filesystem::copy_options::overwrite_existing, ec);
            if (!ec && std::filesystem::exists(updater_tmp)) {
                const std::wstring args =
                    L"--pid " + std::to_wstring(GetCurrentProcessId()) +
                    L" --source \"" + stage.wstring() + L"\"" +
                    L" --target \"" + install_dir.wstring() + L"\"" +
                    L" --relaunch \"" + exe_path.wstring() + L"\"" +
                    L" --cleanup \"" + zip_path.wstring() + L"\"";
                const auto rc = reinterpret_cast<INT_PTR>(ShellExecuteW(
                    nullptr, L"open", updater_tmp.wstring().c_str(), args.c_str(),
                    work.wstring().c_str(), SW_SHOWNORMAL));
                if (rc > 32) {
                    co_await ui_thread;
                    winrt::Microsoft::UI::Xaml::Application::Current().Exit();
                    co_return;
                }
            }
        }

        // Anything went wrong: leave the app running and open the download page.
        co_await ui_thread;
        ShellExecuteW(nullptr, L"open", kReleasesLatestUrl, nullptr, nullptr, SW_SHOWNORMAL);
    }

    // Runs once on launch: if a newer release exists, offers Install/Later.
    // Shows nothing when up to date or offline, so it never blocks a normal
    // launch (dev/CI builds are at or ahead of the published release).
    winrt::fire_and_forget PromptForUpdateOnLaunchAsync(winrt::Microsoft::UI::Xaml::XamlRoot xaml_root)
    {
        namespace Controls = winrt::Microsoft::UI::Xaml::Controls;
        auto ui_thread = winrt::apartment_context();

        const winrt::hstring json = co_await FetchReleaseJsonAsync();
        std::string latest;
        bool available = false;
        if (!json.empty()) {
            const std::string body = winrt::to_string(json);
            const WbUpdateCheck check =
                WinBlaze::UI::NativeBridge::CheckForUpdate(kCurrentVersion, body);
            if (check.parsed != 0 && check.available != 0) {
                available = true;
                latest.assign(reinterpret_cast<const char*>(check.latest), check.latest_len);
            }
        }
        if (!available) {
            co_return;
        }

        co_await ui_thread;
        const std::wstring latest_w(winrt::to_hstring(latest).c_str());
        const std::wstring current(winrt::to_hstring(kCurrentVersion).c_str());
        Controls::ContentDialog dialog;
        dialog.XamlRoot(xaml_root);
        dialog.Title(winrt::box_value(winrt::hstring(L"Update available")));
        dialog.Content(winrt::box_value(winrt::hstring(
            L"WinBlaze " + latest_w + L" is available - you have " + current +
            L". Install it now? WinBlaze will download the update, close, and reopen.")));
        dialog.PrimaryButtonText(L"Install now");
        dialog.SecondaryButtonText(L"Open download page");
        dialog.CloseButtonText(L"Later");
        dialog.DefaultButton(Controls::ContentDialogButton::Primary);
        try {
            const auto result = co_await dialog.ShowAsync();
            if (result == Controls::ContentDialogResult::Primary) {
                InstallUpdateAsync(latest_w);
            }
            else if (result == Controls::ContentDialogResult::Secondary) {
                ShellExecuteW(nullptr, L"open", kReleasesLatestUrl, nullptr, nullptr, SW_SHOWNORMAL);
            }
        }
        catch (...) {
            // A dialog can throw if one is already open; ignore.
        }
    }

    bool IsProcessElevated()
    {
        static const bool elevated = [] {
            HANDLE token = nullptr;
            if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) {
                return false;
            }
            TOKEN_ELEVATION elevation{};
            DWORD size = 0;
            const bool queried = GetTokenInformation(
                token, TokenElevation, &elevation, sizeof(elevation), &size) != 0;
            CloseHandle(token);
            return queried && elevation.TokenIsElevated != 0;
        }();
        return elevated;
    }

    /// Relaunches this executable through the UAC consent prompt; returns
    /// true when the elevated instance started (the caller should exit).
    bool RelaunchElevated()
    {
        wchar_t module_path[MAX_PATH]{};
        if (GetModuleFileNameW(nullptr, module_path, MAX_PATH) == 0) {
            return false;
        }
        const auto result = reinterpret_cast<INT_PTR>(ShellExecuteW(
            nullptr, L"runas", module_path, nullptr, nullptr, SW_SHOWNORMAL));
        return result > 32;
    }

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

    std::wstring FormatFileTimeUtc(int64_t file_time_ticks)
    {
        if (file_time_ticks <= 0) {
            return L"-";
        }

        const __time64_t unix_seconds =
            (file_time_ticks / kFileTimeTicksPerSecond) - kUnixToFileTimeSeconds;
        std::tm utc_tm{};
        if (_gmtime64_s(&utc_tm, &unix_seconds) != 0) {
            return L"-";
        }

        wchar_t buffer[32];
        swprintf_s(
            buffer,
            L"%04d-%02d-%02d %02d:%02d",
            utc_tm.tm_year + 1900,
            utc_tm.tm_mon + 1,
            utc_tm.tm_mday,
            utc_tm.tm_hour,
            utc_tm.tm_min);
        return buffer;
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
        case 16:
            return L"Fast scan unavailable";
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

        // High Velocity shell: slim sidebar column on the left, then the
        // workspace (top bar row + content row) and a terminal-style status
        // strip across the bottom.
        auto menu_row_def = RowDefinition();
        menu_row_def.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto content_row_def = RowDefinition();
        content_row_def.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star));
        auto footer_row_def = RowDefinition();
        footer_row_def.Height(GridLengthHelper::FromValueAndType(32.0, GridUnitType::Pixel));

        root.RowDefinitions().Append(menu_row_def);
        root.RowDefinitions().Append(content_row_def);
        root.RowDefinitions().Append(footer_row_def);

        auto sidebar_col_def = ColumnDefinition();
        sidebar_col_def.Width(GridLengthHelper::FromValueAndType(184.0, GridUnitType::Pixel));
        auto main_col_def = ColumnDefinition();
        main_col_def.Width(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star));
        root.ColumnDefinitions().Append(sidebar_col_def);
        root.ColumnDefinitions().Append(main_col_def);

        // --- Sidebar (High Velocity navigation rail) ---
        {
            auto sidebar_host = Border{};
            sidebar_host.Background(make_brush(theme.chip_background));
            sidebar_host.BorderBrush(make_brush(theme.card_border));
            sidebar_host.BorderThickness(Thickness{ 0.0, 0.0, 1.0, 0.0 });

            auto sidebar_grid = Grid{};
            auto sidebar_top_row = RowDefinition();
            sidebar_top_row.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
            auto sidebar_fill_row = RowDefinition();
            sidebar_fill_row.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star));
            auto sidebar_bottom_row = RowDefinition();
            sidebar_bottom_row.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
            sidebar_grid.RowDefinitions().Append(sidebar_top_row);
            sidebar_grid.RowDefinitions().Append(sidebar_fill_row);
            sidebar_grid.RowDefinitions().Append(sidebar_bottom_row);

            auto sidebar_stack = StackPanel{};
            sidebar_stack.Padding(Thickness{ 14.0, 16.0, 14.0, 8.0 });
            sidebar_stack.Spacing(4.0);

            auto wordmark = TextBlock{};
            wordmark.Text(L"WINBLAZE");
            wordmark.FontSize(17.0);
            wordmark.FontWeight({ 800 });
            wordmark.CharacterSpacing(120);
            wordmark.Foreground(make_brush(theme.chip_active_background));
            sidebar_stack.Children().Append(wordmark);

            auto version_caption = TextBlock{};
            version_caption.Text(L"v2.0 high-velocity");
            version_caption.FontSize(10.0);
            version_caption.FontFamily(Microsoft::UI::Xaml::Media::FontFamily(L"Cascadia Mono, Consolas"));
            version_caption.Foreground(make_brush(theme.text_secondary));
            version_caption.Margin(Thickness{ 1.0, 0.0, 0.0, 14.0 });
            sidebar_stack.Children().Append(version_caption);

            auto make_sidebar_item = [&](std::wstring_view label, bool active) {
                auto item = Button{};
                item.Content(box_value(winrt::hstring(label)));
                item.HorizontalAlignment(HorizontalAlignment::Stretch);
                item.HorizontalContentAlignment(HorizontalAlignment::Left);
                item.Padding(Thickness{ 12.0, 8.0, 12.0, 8.0 });
                item.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
                item.BorderThickness(Thickness{ 0.0, 0.0, 0.0, 0.0 });
                item.FontSize(12.0);
                item.FontFamily(Microsoft::UI::Xaml::Media::FontFamily(L"Cascadia Mono, Consolas"));
                if (active) {
                    item.Background(make_brush(theme.chip_active_background));
                    item.Foreground(make_brush(theme.text_on_accent));
                    item.FontWeight({ 700 });
                } else {
                    item.Background(make_brush(Windows::UI::Colors::Transparent()));
                    item.Foreground(make_brush(theme.text_secondary));
                }
                Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                    item, winrt::hstring(label));
                sidebar_stack.Children().Append(item);
                return item;
            };

            auto register_view_item = [&](std::wstring_view label, AppView view, bool active) {
                auto item = make_sidebar_item(label, active);
                item.Click([this, view](auto const&, auto const&) { SwitchView(view); });
                m_sidebar_items[view] = item;
            };
            register_view_item(L"Dashboard", AppView::Dashboard, false);
            register_view_item(L"Explorer", AppView::Explorer, true);
            register_view_item(L"Insights", AppView::Insights, false);
            register_view_item(L"Cleanup", AppView::Cleanup, false);

            Grid::SetRow(sidebar_stack, 0);
            sidebar_grid.Children().Append(sidebar_stack);

            auto sidebar_bottom = StackPanel{};
            sidebar_bottom.Padding(Thickness{ 14.0, 8.0, 14.0, 12.0 });
            sidebar_bottom.Spacing(6.0);

            auto make_sidebar_caption_item = [&](std::wstring_view label, AppView view) {
                auto item = Button{};
                item.Content(box_value(winrt::hstring(label)));
                item.HorizontalAlignment(HorizontalAlignment::Stretch);
                item.HorizontalContentAlignment(HorizontalAlignment::Left);
                item.Padding(Thickness{ 4.0, 4.0, 4.0, 4.0 });
                item.BorderThickness(Thickness{ 0.0, 0.0, 0.0, 0.0 });
                item.Background(make_brush(Windows::UI::Colors::Transparent()));
                item.Foreground(make_brush(theme.text_secondary));
                item.FontSize(11.0);
                item.FontFamily(Microsoft::UI::Xaml::Media::FontFamily(L"Cascadia Mono, Consolas"));
                Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(item, winrt::hstring(label));
                item.Click([this, view](auto const&, auto const&) { SwitchView(view); });
                m_sidebar_items[view] = item;
                sidebar_bottom.Children().Append(item);
            };
            make_sidebar_caption_item(L"SETTINGS", AppView::Settings);
            make_sidebar_caption_item(L"SUPPORT", AppView::Support);

            m_sidebar_status_text = TextBlock{};
            m_sidebar_status_text.Text(L"ENGINE: idle");
            m_sidebar_status_text.FontSize(10.0);
            m_sidebar_status_text.FontFamily(Microsoft::UI::Xaml::Media::FontFamily(L"Cascadia Mono, Consolas"));
            m_sidebar_status_text.Foreground(make_brush(theme.folder_accent));
            m_sidebar_status_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_sidebar_status_text.Margin(Thickness{ 0.0, 8.0, 0.0, 0.0 });
            sidebar_bottom.Children().Append(m_sidebar_status_text);

            Grid::SetRow(sidebar_bottom, 2);
            sidebar_grid.Children().Append(sidebar_bottom);

            sidebar_host.Child(sidebar_grid);
            Grid::SetRow(sidebar_host, 0);
            Grid::SetRowSpan(sidebar_host, 2);
            Grid::SetColumn(sidebar_host, 0);
            root.Children().Append(sidebar_host);
        }

        // Menu bar
        auto menu_host = Border{};
        menu_host.Background(make_brush(theme.card_background));
        menu_host.BorderBrush(make_brush(theme.card_border));
        menu_host.BorderThickness(Thickness{ 0.0, 0.0, 0.0, 1.0 });

        // Flat buttons with attached MenuFlyouts rather than a MenuBar
        // control: this app runs without XamlControlsResources (App.xaml
        // fails to load — see failures.jsonl), so template-heavy controls
        // like MenuBar throw a stowed exception on first layout.
        auto menu_row = StackPanel{};
        menu_row.Orientation(Orientation::Horizontal);
        menu_row.Spacing(4.0);
        menu_row.Padding(Thickness{ 8.0, 2.0, 8.0, 2.0 });


        auto make_menu_button = [&](std::wstring_view title, std::wstring_view automation_name) {
            auto button = Button{};
            button.Content(box_value(winrt::hstring(title)));
            button.Background(make_brush(Windows::UI::Colors::Transparent()));
            button.BorderThickness(Thickness{ 0.0, 0.0, 0.0, 0.0 });
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                button, winrt::hstring(automation_name));
            menu_row.Children().Append(button);
            return button;
        };

        auto file_button = make_menu_button(L"File", L"File menu");
        auto file_flyout = MenuFlyout{};

        auto select_drive_item = MenuFlyoutItem{};
        select_drive_item.Text(L"Select drive...");
        select_drive_item.Click([this](auto const&, auto const&) {
            FocusRootPathBox();
        });
        file_flyout.Items().Append(select_drive_item);

        auto rescan_item = MenuFlyoutItem{};
        rescan_item.Text(L"Rescan");
        rescan_item.Click([this](auto const&, auto const&) {
            BeginScanFromCurrentRoot();
        });
        file_flyout.Items().Append(rescan_item);

        auto exit_item = MenuFlyoutItem{};
        exit_item.Text(L"Exit");
        exit_item.Click([this](auto const&, auto const&) {
            Close();
        });
        file_flyout.Items().Append(exit_item);
        file_button.Flyout(file_flyout);

        auto help_button = make_menu_button(L"Help", L"Help menu");
        auto help_flyout = MenuFlyout{};
        auto about_item = MenuFlyoutItem{};
        about_item.Text(L"About WinBlaze");
        about_item.Click([this](auto const&, auto const&) {
            OpenExternal(L"https://github.com/marksmayo/WinBlaze");
            UpdateStatus(L"Opened the WinBlaze repository.");
        });
        help_flyout.Items().Append(about_item);
        help_button.Flyout(help_flyout);

        menu_host.Child(menu_row);
        Grid::SetRow(menu_host, 0);
        Grid::SetColumn(menu_host, 1);
        root.Children().Append(menu_host);

        // Status bar: scan status, progress with elapsed time, and the
        // current selection summary.
        auto footer_bar = Border{};
        footer_bar.Background(make_brush(theme.app_background));
        footer_bar.BorderBrush(make_brush(theme.card_border));
        footer_bar.BorderThickness(Thickness{ 0.0, 1.0, 0.0, 0.0 });
        footer_bar.Height(32.0);
        footer_bar.Padding(Thickness{ 16.0, 0.0, 16.0, 0.0 });

        auto footer_stack = StackPanel{};
        footer_stack.Orientation(Orientation::Horizontal);
        footer_stack.Spacing(24);
        footer_stack.VerticalAlignment(VerticalAlignment::Center);
        footer_bar.Child(footer_stack);

        m_status_text = TextBlock{};
        m_status_text.Text(L"Idle");
        m_status_text.FontSize(12.0);
        m_status_text.Foreground(make_brush(theme.text_primary));
        m_status_text.VerticalAlignment(VerticalAlignment::Center);
        footer_stack.Children().Append(m_status_text);

        m_progress_text = TextBlock{};
        m_progress_text.Text(L"0% complete");
        m_progress_text.FontSize(12.0);
        m_progress_text.Foreground(make_brush(theme.text_secondary));
        m_progress_text.VerticalAlignment(VerticalAlignment::Center);
        footer_stack.Children().Append(m_progress_text);

        auto progress_track = Border{};
        progress_track.Width(150.0);
        progress_track.Height(6.0);
        progress_track.VerticalAlignment(VerticalAlignment::Center);
        progress_track.Background(make_brush(theme.progress_track));
        progress_track.CornerRadius(UniformRadius(theme.progress_radius));

        m_scan_progress_fill = Border{};
        m_scan_progress_fill.Width(0.0);
        m_scan_progress_fill.Height(6.0);
        m_scan_progress_fill.HorizontalAlignment(HorizontalAlignment::Left);
        m_scan_progress_fill.Background(make_brush(theme.progress_fill));
        m_scan_progress_fill.CornerRadius(UniformRadius(theme.progress_radius));
        progress_track.Child(m_scan_progress_fill);
        footer_stack.Children().Append(progress_track);

        m_selection_status_text = TextBlock{};
        m_selection_status_text.FontSize(12.0);
        m_selection_status_text.Foreground(make_brush(theme.text_secondary));
        m_selection_status_text.VerticalAlignment(VerticalAlignment::Center);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_selection_status_text,
            L"Selection summary");
        footer_stack.Children().Append(m_selection_status_text);

        Grid::SetRow(footer_bar, 2);
        Grid::SetColumnSpan(footer_bar, 2);
        root.Children().Append(footer_bar);

        // Create the scrollable main content Grid
        auto shell = Grid{};
        shell.Margin(Thickness{ 20.0, 18.0, 20.0, 20.0 });
        shell.RowSpacing(12.0);
        shell.ColumnSpacing(12.0);

        // Rows inside content grid. The tree/extension row and the treemap
        // row share the leftover height (WinDirStat proportions); everything
        // else sizes to content.
        auto shell_r0 = RowDefinition();
        shell_r0.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto shell_r1 = RowDefinition();
        shell_r1.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto shell_r2 = RowDefinition();
        shell_r2.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star)); // Directory table
        shell_r2.MinHeight(180.0);
        auto shell_r3 = RowDefinition();
        // The treemap is the hero visual: give it the larger share.
        shell_r3.Height(GridLengthHelper::FromValueAndType(1.6, GridUnitType::Star)); // Treemap | extensions
        shell_r3.MinHeight(260.0);
        auto shell_r4 = RowDefinition();
        shell_r4.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto shell_r5 = RowDefinition();
        shell_r5.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto shell_r6 = RowDefinition();
        shell_r6.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));

        shell.RowDefinitions().Append(shell_r0);
        shell.RowDefinitions().Append(shell_r1);
        shell.RowDefinitions().Append(shell_r2);
        shell.RowDefinitions().Append(shell_r3);
        shell.RowDefinitions().Append(shell_r4);
        shell.RowDefinitions().Append(shell_r5);
        shell.RowDefinitions().Append(shell_r6);

        // Columns inside content grid: Column 0: Width 2*, Column 1: Width 1*
        auto shell_c0 = ColumnDefinition();
        shell_c0.Width(GridLengthHelper::FromValueAndType(2.0, GridUnitType::Star));
        auto shell_c1 = ColumnDefinition();
        shell_c1.Width(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star));

        shell.ColumnDefinitions().Append(shell_c0);
        shell.ColumnDefinitions().Append(shell_c1);

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
            m_start_scan_button.Background(make_brush(theme.chip_active_background));
            m_start_scan_button.Foreground(make_brush(theme.text_on_accent));
            m_start_scan_button.FontWeight({ 700 });
            m_start_scan_button.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
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

            // Reveal buttons for the secondary panels. Show-only (a second
            // click does not hide) so automation scripts can invoke them
            // idempotently; the panels' own checkboxes hide them again.
            auto search_reveal_button = Button{};
            search_reveal_button.Content(box_value(L"Search"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                search_reveal_button,
                L"Search");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                search_reveal_button,
                L"Show the search panel. Ctrl+4 also opens it.");
            search_reveal_button.Click([this](auto const&, auto const&) {
                m_show_search = true;
                SetSection(m_active_section);
            });
            root_row.Children().Append(search_reveal_button);

            auto diagnostics_reveal_button = Button{};
            diagnostics_reveal_button.Content(box_value(L"Diagnostics"));
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                diagnostics_reveal_button,
                L"Diagnostics");
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
                diagnostics_reveal_button,
                L"Show the runtime diagnostics panel. Ctrl+5 also opens it.");
            diagnostics_reveal_button.Click([this](auto const&, auto const&) {
                m_show_runtime_metrics = true;
                if (m_runtime_metrics_toggle) {
                    m_runtime_metrics_toggle.IsChecked(true);
                }
                SetSection(m_active_section);
            });
            root_row.Children().Append(diagnostics_reveal_button);

            scan_stack.Children().Append(root_row);

            Grid::SetRow(scan_card, 0);
            Grid::SetColumn(scan_card, 0);
            Grid::SetColumnSpan(scan_card, 2);
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

            Grid::SetRow(banner_stack, 1);
            Grid::SetColumn(banner_stack, 0);
            Grid::SetColumnSpan(banner_stack, 2);
            shell.Children().Append(banner_stack);
        }
        TraceStartup(L"BuildShell after state banners");

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

        Grid::SetRow(summary_card, 6);
        Grid::SetColumn(summary_card, 0);
        Grid::SetColumnSpan(summary_card, 2);
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

            Grid::SetRow(search_card, 4);
            Grid::SetColumn(search_card, 0);
            Grid::SetColumnSpan(search_card, 2);
            shell.Children().Append(search_card);
        }
        TraceStartup(L"BuildShell search card end");

        {
            auto tree_card = Border{};
            m_tree_card = tree_card;
            apply_card_style(tree_card);

            // Grid so the tree ListView stretches to fill the card.
            auto tree_stack = Grid{};
            tree_stack.RowSpacing(8);
            auto append_tree_row = [&](Microsoft::UI::Xaml::UIElement const& element, bool star) {
                auto row = RowDefinition();
                row.Height(star
                    ? GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star)
                    : GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
                tree_stack.RowDefinitions().Append(row);
                Grid::SetRow(element.as<Microsoft::UI::Xaml::FrameworkElement>(), tree_stack.RowDefinitions().Size() - 1);
                tree_stack.Children().Append(element);
            };
            tree_card.Child(tree_stack);

            append_tree_row(make_card_title(L"Directory analysis"), false);

            auto tree_action_row = StackPanel{};
            tree_action_row.Orientation(Orientation::Horizontal);
            tree_action_row.Spacing(10);

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
            tree_action_hint.Text(L"Double-click folders to expand; rows page in a virtualized window.");
            tree_action_hint.Opacity(0.72);
            tree_action_hint.VerticalAlignment(VerticalAlignment::Center);
            tree_action_row.Children().Append(tree_action_hint);
            append_tree_row(tree_action_row, false);

            m_tree_catalog.clear();
            m_tree_catalog_keys.clear();

            m_tree_list_status_text = TextBlock{};
            m_tree_list_status_text.Text(L"Scan or load a cached catalog to build the folder tree; rows render in virtualized ListView containers.");
            m_tree_list_status_text.Opacity(0.72);
            m_tree_list_status_text.TextWrapping(TextWrapping::WrapWholeWords);
            append_tree_row(m_tree_list_status_text, false);

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
            // Widths mirror CreateTreeNodeListItem's row cells: expander
            // glyph (14) + spacing + name block (300) collapse into one Name
            // column; the remaining columns match the row cell min-widths.
            tree_list_header.Children().Append(make_tree_header_label(L"Name", 326.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Usage", 150.0));
            tree_list_header.Children().Append(make_tree_header_label(L"%", 52.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Physical", 84.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Logical", 84.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Files", 64.0));
            tree_list_header.Children().Append(make_tree_header_label(L"Last Change", 120.0));
            append_tree_row(tree_list_header, false);

            m_tree_list_view = ListView{};
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
            append_tree_row(m_tree_list_view, true);
            m_tree_updates_ready = true;

            // Directory table spans the full width per the design; the
            // treemap + extension panel share the row below.
            Grid::SetRow(tree_card, 2);
            Grid::SetColumn(tree_card, 0);
            Grid::SetColumnSpan(tree_card, 2);
            shell.Children().Append(tree_card);
        }
        TraceStartup(L"BuildShell tree card end");

        auto extension_card = Border{};
        m_extension_card = extension_card;
        apply_card_style(extension_card);

        auto extension_stack = StackPanel{};
        extension_stack.Spacing(6);
        extension_card.Child(extension_stack);

        extension_stack.Children().Append(make_card_title(L"Extensions"));

        auto extension_subtitle = TextBlock{};
        extension_subtitle.Text(L"Bytes and file counts by extension, aggregated across the whole scan.");
        extension_subtitle.Opacity(0.75);
        extension_subtitle.TextWrapping(TextWrapping::WrapWholeWords);
        extension_stack.Children().Append(extension_subtitle);

        auto extension_list_header = StackPanel{};
        extension_list_header.Orientation(Orientation::Horizontal);
        extension_list_header.Spacing(10);
        extension_list_header.Margin(Thickness{ 12.0, 4.0, 0.0, 0.0 });
        auto make_extension_header_label = [&](std::wstring_view text, double width) {
            auto label = TextBlock{};
            label.Text(winrt::hstring(text));
            label.Width(width);
            label.Opacity(0.68);
            label.Foreground(make_brush(theme.text_primary));
            return label;
        };
        extension_list_header.Children().Append(make_extension_header_label(L"", 14.0));
        extension_list_header.Children().Append(make_extension_header_label(L"Ext", 56.0));
        extension_list_header.Children().Append(make_extension_header_label(L"Description", 168.0));
        extension_list_header.Children().Append(make_extension_header_label(L"Bytes", 72.0));
        extension_list_header.Children().Append(make_extension_header_label(L"%", 44.0));
        extension_list_header.Children().Append(make_extension_header_label(L"Files", 56.0));
        extension_stack.Children().Append(extension_list_header);

        m_extension_list_view = ListView{};
        m_extension_list_view.MaxHeight(440.0);
        m_extension_list_view.ItemsPanel(
            Microsoft::UI::Xaml::Markup::XamlReader::Load(
                LR"(<ItemsPanelTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><ItemsStackPanel Orientation="Vertical"/></ItemsPanelTemplate>)")
                .as<Microsoft::UI::Xaml::Controls::ItemsPanelTemplate>());
        m_extension_list_view.SelectionMode(ListViewSelectionMode::None);
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            m_extension_list_view,
            L"Extension breakdown");
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(
            m_extension_list_view,
            L"Per-extension bytes and file counts aggregated across the whole scan, sorted by bytes descending.");
        extension_stack.Children().Append(m_extension_list_view);

        m_extension_list_status_text = TextBlock{};
        m_extension_list_status_text.Text(L"Waiting for scan data.");
        m_extension_list_status_text.Opacity(0.72);
        m_extension_list_status_text.TextWrapping(TextWrapping::WrapWholeWords);
        extension_stack.Children().Append(m_extension_list_status_text);
        TraceStartup(L"BuildShell extension card end");

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
        TraceStartup(L"BuildShell detail card end");

        // Right pane beside the folder tree: extension legend fills the
        // height; the detail card (hidden by default) sits above it when
        // enabled via its view toggle.
        auto right_content_panel = Grid{};
        right_content_panel.RowSpacing(12.0);
        auto right_detail_row = RowDefinition();
        right_detail_row.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
        auto right_extension_row = RowDefinition();
        right_extension_row.Height(GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star));
        right_content_panel.RowDefinitions().Append(right_detail_row);
        right_content_panel.RowDefinitions().Append(right_extension_row);
        Grid::SetRow(detail_card, 0);
        right_content_panel.Children().Append(detail_card);
        Grid::SetRow(extension_card, 1);
        right_content_panel.Children().Append(extension_card);
        Grid::SetRow(right_content_panel, 3);
        Grid::SetColumn(right_content_panel, 1);
        shell.Children().Append(right_content_panel);

        {
            auto treemap_card = Border{};
            m_treemap_card = treemap_card;
            apply_card_style(treemap_card);

            // Grid instead of StackPanel so the SwapChainPanel stretches to
            // fill the card's star-sized row.
            auto treemap_stack = Grid{};
            treemap_stack.RowSpacing(8);
            auto append_treemap_row = [&](Microsoft::UI::Xaml::UIElement const& element, bool star) {
                auto row = RowDefinition();
                row.Height(star
                    ? GridLengthHelper::FromValueAndType(1.0, GridUnitType::Star)
                    : GridLengthHelper::FromValueAndType(1.0, GridUnitType::Auto));
                treemap_stack.RowDefinitions().Append(row);
                Grid::SetRow(element.as<Microsoft::UI::Xaml::FrameworkElement>(), treemap_stack.RowDefinitions().Size() - 1);
                treemap_stack.Children().Append(element);
            };
            treemap_card.Child(treemap_stack);

            append_treemap_row(make_card_title(L"Treemap"), false);

            auto treemap_subtitle = TextBlock{};
            treemap_subtitle.Text(L"Scan or load a cached catalog to render proportional usage tiles.");
            treemap_subtitle.Opacity(0.75);
            treemap_subtitle.TextWrapping(TextWrapping::WrapWholeWords);
            append_treemap_row(treemap_subtitle, false);

            m_treemap_surface = SwapChainPanel{};
            // Fills the star-sized treemap row; SizeChanged re-renders.
            m_treemap_surface.MinHeight(160.0);
            m_treemap_surface.VerticalAlignment(VerticalAlignment::Stretch);
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
            append_treemap_row(m_treemap_surface, true);

            m_treemap_render_status = ProbeTreemapRenderStack();
            m_treemap_surface_status_text = TextBlock{};
            m_treemap_surface_status_text.Text(winrt::hstring(
                L"SwapChainPanel host is initialized. " + m_treemap_render_status));
            m_treemap_surface_status_text.Opacity(0.72);
            m_treemap_surface_status_text.TextWrapping(TextWrapping::WrapWholeWords);
            append_treemap_row(m_treemap_surface_status_text, false);

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
            append_treemap_row(m_treemap_zoom_overlay, false);

            Grid::SetRow(treemap_card, 3);
            Grid::SetColumn(treemap_card, 0);
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

            // View-panel toggles (moved here from the removed sidebar).
            auto view_toggle_row = StackPanel{};
            view_toggle_row.Orientation(Orientation::Horizontal);
            view_toggle_row.Spacing(16.0);

            auto make_view_toggle = [&](CheckBox& storage, bool checked, std::wstring_view text, std::wstring_view help_text) {
                auto item = CheckBox{};
                storage = item;
                item.Content(box_value(winrt::hstring(text)));
                item.IsChecked(checked);
                Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(item, winrt::hstring(text));
                Microsoft::UI::Xaml::Automation::AutomationProperties::SetHelpText(item, winrt::hstring(help_text));
                item.Click({ this, &MainWindow::OnOptionalPanelToggleClicked });
                return item;
            };

            view_toggle_row.Children().Append(make_view_toggle(m_current_state_toggle, m_show_current_state, L"Current state", L"Show or hide the current state panel."));
            view_toggle_row.Children().Append(make_view_toggle(m_folder_view_toggle, m_show_folder_view, L"Folder view", L"Show or hide the folder and file detail panel."));
            view_toggle_row.Children().Append(make_view_toggle(m_folder_tree_toggle, m_show_folder_tree, L"Folder tree", L"Show or hide the virtualized folder tree panel."));
            view_toggle_row.Children().Append(make_view_toggle(m_runtime_metrics_toggle, m_show_runtime_metrics, L"Runtime metrics", L"Show or hide runtime metrics at the bottom of the UI."));
            runtime_stack.Children().Append(view_toggle_row);

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

            m_catalog_snapshot_text = TextBlock{};
            m_catalog_snapshot_text.Text(L"Top entries will appear here.");
            m_catalog_snapshot_text.Opacity(0.75);
            m_catalog_snapshot_text.TextWrapping(TextWrapping::WrapWholeWords);
            m_developer_diagnostics_panel.Children().Append(m_catalog_snapshot_text);

            Grid::SetRow(runtime_card, 5);
            Grid::SetColumn(runtime_card, 0);
            Grid::SetColumnSpan(runtime_card, 2);
            shell.Children().Append(runtime_card);
        }
        TraceStartup(L"BuildShell runtime card end");

        // --- Sidebar view hosts (Dashboard/Insights/Cleanup/Settings/
        // Support). Each is a scrollable card covering the explorer pane
        // area; content is rebuilt on switch so data stays fresh.
        {
            auto make_view_card = [&](Microsoft::UI::Xaml::Controls::Border& card_storage,
                                      Microsoft::UI::Xaml::Controls::StackPanel& content_storage) {
                auto card = Border{};
                card_storage = card;
                apply_card_style(card);
                auto scroll = ScrollViewer{};
                scroll.VerticalScrollBarVisibility(ScrollBarVisibility::Auto);
                scroll.HorizontalScrollBarVisibility(ScrollBarVisibility::Disabled);
                auto content = StackPanel{};
                content_storage = content;
                content.Spacing(14.0);
                scroll.Content(content);
                card.Child(scroll);
                card.Visibility(Visibility::Collapsed);
                Grid::SetRow(card, 2);
                Grid::SetRowSpan(card, 5);
                Grid::SetColumn(card, 0);
                Grid::SetColumnSpan(card, 2);
                shell.Children().Append(card);
            };
            make_view_card(m_dashboard_card, m_dashboard_content);
            make_view_card(m_insights_card, m_insights_content);
            make_view_card(m_cleanup_card, m_cleanup_content);
            make_view_card(m_settings_card, m_settings_content);
            make_view_card(m_support_card, m_support_content);
        }

        {
            // The panes size to the window now (star rows); no outer scroll.
            Grid::SetRow(shell, 1);
            Grid::SetColumn(shell, 1);
            root.Children().Append(shell);
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

        // Once per launch, check GitHub for a newer release and (only if one
        // exists) offer an update dialog. Non-blocking; silent when up to date
        // or offline.
        if (!m_update_check_started) {
            m_update_check_started = true;
            if (auto content = Content()) {
                if (auto xaml_root = content.XamlRoot()) {
                    PromptForUpdateOnLaunchAsync(xaml_root);
                }
            }
        }
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
        std::lock_guard guard(m_pending_ui_mutex);
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
            {
                std::lock_guard guard(m_pending_ui_mutex);
                m_last_input_latency_ms = std::chrono::duration<double, std::milli>(
                    std::chrono::steady_clock::now() - input_started).count();
            }
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
        // Drop the previous scan's tree; the live tree rebuilds from
        // directory events as the scan discovers them.
        m_tree_nodes.clear();
        m_tree_visible_rows.clear();
        m_tree_node_index_by_id.clear();
        m_live_orphans.clear();
        m_live_directory_backlog.clear();
        m_live_backlog_cursor = 0;
        m_last_live_tree_refresh = {};
        {
            std::lock_guard guard(m_pending_ui_mutex);
            m_scan_started_at = std::chrono::steady_clock::now();
            m_last_scan_duration_text = L"Scan duration: in progress";
        }
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
        SelectVisualizationTarget(L"Root volume", m_current_root_path, L"Volume", L"0 B");
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

        if (TreeArenaActive()) {
            RefreshTreeListView();
        } else {
            UpdateTreeSnapshotPreview(FilterTreeCatalog());
        }
        UpdateStatus(L"Tree list moved to the previous row window.");
    }

    void MainWindow::OnTreeWindowNextClicked(
        winrt::Windows::Foundation::IInspectable const&,
        Microsoft::UI::Xaml::RoutedEventArgs const&)
    {
        if (TreeArenaActive()) {
            if (m_tree_window_offset + kTreeListVirtualizedWindowLimit < m_tree_visible_rows.size()) {
                m_tree_window_offset += kTreeListVirtualizedWindowLimit;
            }
            RefreshTreeListView();
        } else {
            const auto entries = FilterTreeCatalog();
            if (m_tree_window_offset + kTreeListVirtualizedWindowLimit < entries.size()) {
                m_tree_window_offset += kTreeListVirtualizedWindowLimit;
            }
            UpdateTreeSnapshotPreview(entries);
        }
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
            // Arena-backed tree rows tag with their node index.
            if (auto node_index_ref = item.Tag().try_as<winrt::Windows::Foundation::IReference<uint64_t>>()) {
                const auto node_index = static_cast<size_t>(node_index_ref.Value());
                if (node_index < m_tree_nodes.size() && !m_tree_nodes[node_index].is_more_row) {
                    auto const& node = m_tree_nodes[node_index];
                    SelectVisualizationTarget(
                        node.name,
                        TreeNodePath(node_index),
                        node.is_directory ? L"Folder" : L"File",
                        FormatBytes(node.physical_bytes));
                }
                return;
            }

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

    bool MainWindow::EnsureTreemapRenderStack(int width, int height)
    {
        HRESULT result = S_OK;

        // Device + device-independent resources: built once and reused across
        // every render and resize. The original probe rebuilt all of this
        // (including the D3D device) on each dirty/resize tick.
        if (!m_render_d2d_context) {
            winrt::com_ptr<ID3D11Device> d3d_device;
            winrt::com_ptr<ID3D11DeviceContext> d3d_context;
            D3D_FEATURE_LEVEL selected_level{};
            constexpr D3D_FEATURE_LEVEL levels[] = {
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
            };
            result = ::D3D11CreateDevice(
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
                m_treemap_render_status = L"Treemap render D3D device failed: " + HresultText(result);
                return false;
            }

            winrt::com_ptr<ID2D1Factory3> d2d_factory;
            D2D1_FACTORY_OPTIONS options{};
            result = ::D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                __uuidof(ID2D1Factory3),
                &options,
                reinterpret_cast<void**>(d2d_factory.put()));
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render D2D factory failed: " + HresultText(result);
                return false;
            }

            winrt::com_ptr<IDXGIDevice> dxgi_device;
            result = d3d_device->QueryInterface(__uuidof(IDXGIDevice), dxgi_device.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DXGI device failed: " + HresultText(result);
                return false;
            }

            winrt::com_ptr<ID2D1Device> d2d_device;
            result = d2d_factory->CreateDevice(dxgi_device.get(), d2d_device.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render D2D device failed: " + HresultText(result);
                return false;
            }

            winrt::com_ptr<ID2D1DeviceContext> d2d_context;
            result = d2d_device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, d2d_context.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render D2D context failed: " + HresultText(result);
                return false;
            }

            winrt::com_ptr<IDWriteFactory> dwrite_factory;
            result = ::DWriteCreateFactory(
                DWRITE_FACTORY_TYPE_SHARED,
                __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown**>(dwrite_factory.put()));
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DWrite factory failed: " + HresultText(result);
                return false;
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
                m_treemap_render_status = L"Treemap render label format failed: " + HresultText(result);
                return false;
            }
            label_format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            label_format->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            label_format->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);

            m_render_d3d_device = d3d_device;
            m_render_d2d_factory = d2d_factory;
            m_render_d2d_device = d2d_device;
            m_render_d2d_context = d2d_context;
            m_render_dwrite_factory = dwrite_factory;
            m_render_label_format = label_format;
            m_render_feature_level = selected_level;
            // Force the swapchain to (re)create against the new device.
            m_render_swap_chain = nullptr;
            m_render_target_bitmap = nullptr;
        }

        // Swapchain + target bitmap: rebuilt only when missing or resized.
        if (!m_render_swap_chain || width != m_render_swap_width || height != m_render_swap_height) {
            // Release references to the old back buffer before the swapchain.
            m_render_d2d_context->SetTarget(nullptr);
            m_render_target_bitmap = nullptr;
            m_render_swap_chain = nullptr;

            winrt::com_ptr<IDXGIDevice> dxgi_device;
            result = m_render_d3d_device->QueryInterface(__uuidof(IDXGIDevice), dxgi_device.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DXGI device failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }
            winrt::com_ptr<IDXGIAdapter> adapter;
            result = dxgi_device->GetAdapter(adapter.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DXGI adapter failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }
            winrt::com_ptr<IDXGIFactory2> factory;
            result = adapter->GetParent(__uuidof(IDXGIFactory2), factory.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DXGI factory failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }

            DXGI_SWAP_CHAIN_DESC1 desc{};
            desc.Width = static_cast<UINT>((std::max)(1, width));
            desc.Height = static_cast<UINT>((std::max)(1, height));
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
                m_render_d3d_device.get(),
                &desc,
                nullptr,
                swap_chain.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render swap-chain creation failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }

            winrt::com_ptr<ID3D11Texture2D> back_buffer;
            result = swap_chain->GetBuffer(0, __uuidof(ID3D11Texture2D), back_buffer.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render back-buffer failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }
            winrt::com_ptr<IDXGISurface> dxgi_surface;
            result = back_buffer->QueryInterface(__uuidof(IDXGISurface), dxgi_surface.put_void());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render DXGI surface failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }

            const auto bitmap_properties = D2D1::BitmapProperties1(
                D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE),
                96.0f,
                96.0f);
            winrt::com_ptr<ID2D1Bitmap1> target_bitmap;
            result = m_render_d2d_context->CreateBitmapFromDxgiSurface(
                dxgi_surface.get(),
                &bitmap_properties,
                target_bitmap.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render D2D target failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }

            // Bind the swapchain to the panel once, when it is (re)created.
            auto panel_native = TreemapSurface().try_as<ISwapChainPanelNative>();
            if (!panel_native) {
                m_treemap_render_status = L"Treemap render panel interface unavailable.";
                ResetTreemapRenderStack();
                return false;
            }
            result = panel_native->SetSwapChain(swap_chain.get());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap render panel bind failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return false;
            }

            m_render_swap_chain = swap_chain;
            m_render_target_bitmap = target_bitmap;
            m_render_swap_width = width;
            m_render_swap_height = height;
        }

        return true;
    }

    void MainWindow::ResetTreemapRenderStack()
    {
        if (m_render_d2d_context) {
            m_render_d2d_context->SetTarget(nullptr);
        }
        m_render_target_bitmap = nullptr;
        m_render_swap_chain = nullptr;
        m_render_label_format = nullptr;
        m_render_dwrite_factory = nullptr;
        m_render_d2d_context = nullptr;
        m_render_d2d_device = nullptr;
        m_render_d2d_factory = nullptr;
        m_render_d3d_device = nullptr;
        m_render_swap_width = 0;
        m_render_swap_height = 0;
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
            if (!EnsureTreemapRenderStack(width, height)) {
                return;
            }
            // Cheap AddRef'd handles into the cached stack; the draw code below
            // uses these names unchanged. selected_level/result are kept for the
            // downstream status string and D2D calls.
            auto d2d_context = m_render_d2d_context;
            auto label_format = m_render_label_format;
            auto swap_chain = m_render_swap_chain;
            const D3D_FEATURE_LEVEL selected_level = m_render_feature_level;
            HRESULT result = S_OK;

            d2d_context->SetTarget(m_render_target_bitmap.get());
            d2d_context->BeginDraw();
            d2d_context->Clear(D2D1::ColorF(0.051f, 0.082f, 0.082f, 1.0f)); // #0d1515 surface

            struct DrawTile
            {
                float left;
                float top;
                float right;
                float bottom;
                D2D1_COLOR_F color;
                std::wstring label;
                bool frame{ false };
            };

            const float surface_width = static_cast<float>(width);
            const float surface_height = static_cast<float>(height);
            std::vector<DrawTile> tiles;
            std::vector<TreemapTileLayout> layout;
            std::wstring layout_name;
            size_t directories_recursed = 0;
            bool needs_deepening = false;

            if (TreeArenaActive()) {
                // Hierarchical squarified layout over the display-tree arena:
                // recurse into directories while their tile stays legible,
                // color files by extension (matching the legend swatches),
                // draw directories as thin frames around their children.
                layout_name = L"squarified";
                constexpr size_t kTileBudget = 20000;
                constexpr float kMinTileDim = 6.0f;
                constexpr float kRecurseMinDim = 28.0f;
                // Budget measured in nodes created, not fetch calls: one
                // directory fetch can populate up to 4096 nodes, so a
                // call-count budget still allowed multi-second stalls.
                const size_t node_budget_limit = m_tree_nodes.size() + 2500;
                const D2D1_COLOR_F folder_fill = D2D1::ColorF(0.098f, 0.129f, 0.133f, 1.0f);  // #192122
                const D2D1_COLOR_F folder_frame = D2D1::ColorF(0.031f, 0.059f, 0.063f, 1.0f); // #080f10

                auto to_d2d_color = [](winrt::Windows::UI::Color const& color) {
                    return D2D1::ColorF(
                        static_cast<float>(color.R) / 255.0f,
                        static_cast<float>(color.G) / 255.0f,
                        static_cast<float>(color.B) / 255.0f,
                        1.0f);
                };

                auto emit_tile = [&](size_t node_index, float left, float top, float right, float bottom, bool frame, bool labeled) {
                    auto const& node = m_tree_nodes[node_index];
                    D2D1_COLOR_F color = frame ? folder_frame : folder_fill;
                    if (!frame && !node.is_directory) {
                        color = to_d2d_color(ExtensionSwatchColor(ExtensionKeyFromName(node.name)));
                    }
                    tiles.push_back(DrawTile{
                        left,
                        top,
                        right,
                        bottom,
                        color,
                        labeled ? node.name : std::wstring{},
                        frame,
                    });
                    layout.push_back(TreemapTileLayout{
                        left,
                        top,
                        right,
                        bottom,
                        node.name,
                        TreeNodePath(node_index),
                        node.is_directory ? std::wstring(L"Folder") : std::wstring(L"File"),
                        FormatBytes(node.physical_bytes),
                    });
                };

                struct QueueItem
                {
                    size_t node;
                    float left;
                    float top;
                    float right;
                    float bottom;
                    uint32_t depth;
                };
                std::vector<QueueItem> queue;
                queue.push_back(QueueItem{ 0, 2.0f, 2.0f, surface_width - 2.0f, surface_height - 2.0f, 0 });

                while (!queue.empty() && tiles.size() < kTileBudget) {
                    const QueueItem item = queue.back();
                    queue.pop_back();
                    const float item_width = item.right - item.left;
                    const float item_height = item.bottom - item.top;
                    if (item_width < kMinTileDim || item_height < kMinTileDim) {
                        continue;
                    }

                    // m_tree_nodes may reallocate inside
                    // EnsureTreeChildrenLoaded — index, don't hold references.
                    const bool is_directory =
                        m_tree_nodes[item.node].is_directory && !m_tree_nodes[item.node].is_more_row;
                    if (!is_directory || item_width < kRecurseMinDim || item_height < kRecurseMinDim) {
                        emit_tile(item.node, item.left, item.top, item.right, item.bottom, false, item.depth <= 1);
                        continue;
                    }

                    // Fetching children mid-paint crosses the FFI and can
                    // populate thousands of nodes; unbounded it stalls the UI
                    // thread for seconds on the first post-scan render.
                    // Budget the fetches and refine over subsequent frames.
                    if (!m_tree_nodes[item.node].children_loaded) {
                        if (m_tree_nodes.size() >= node_budget_limit) {
                            emit_tile(item.node, item.left, item.top, item.right, item.bottom, false, item.depth <= 1);
                            needs_deepening = true;
                            continue;
                        }
                        EnsureTreeChildrenLoaded(item.node);
                    }
                    std::vector<size_t> children;
                    double child_total = 0.0;
                    for (size_t child : m_tree_nodes[item.node].children) {
                        auto const& child_node = m_tree_nodes[child];
                        if (child_node.is_more_row || child_node.physical_bytes == 0) {
                            continue;
                        }
                        children.push_back(child);
                        child_total += static_cast<double>(child_node.physical_bytes);
                    }
                    if (children.empty() || child_total <= 0.0) {
                        emit_tile(item.node, item.left, item.top, item.right, item.bottom, false, item.depth <= 1);
                        continue;
                    }
                    ++directories_recursed;

                    // Directories big enough to caption reserve a header
                    // strip and lay children below it, so the parent label
                    // never draws over the first child's label.
                    const bool labeled_frame =
                        item.depth <= 1 && item_width >= 64.0f && item_height >= 48.0f;
                    constexpr float kHeaderStrip = 19.0f;

                    // Frame drawn behind the children so the directory still
                    // registers hover/tap hits along its 1px border.
                    emit_tile(item.node, item.left, item.top, item.right, item.bottom, true, labeled_frame);

                    // Weight-balanced binary subdivision of this directory's
                    // rectangle (children arrive sorted largest-first).
                    struct Slice
                    {
                        size_t begin;
                        size_t end;
                        float left;
                        float top;
                        float right;
                        float bottom;
                    };
                    auto slice_weight = [&](size_t begin, size_t end) {
                        double total = 0.0;
                        for (size_t index = begin; index < end; ++index) {
                            total += static_cast<double>(m_tree_nodes[children[index]].physical_bytes);
                        }
                        return total;
                    };

                    const float inset = 1.0f;
                    const float children_top =
                        labeled_frame ? item.top + kHeaderStrip : item.top + inset;
                    std::vector<Slice> slices;
                    slices.push_back(Slice{
                        0,
                        children.size(),
                        item.left + inset,
                        children_top,
                        item.right - inset,
                        item.bottom - inset,
                    });
                    while (!slices.empty()) {
                        const Slice slice = slices.back();
                        slices.pop_back();
                        if (slice.begin >= slice.end) {
                            continue;
                        }
                        const float slice_width = slice.right - slice.left;
                        const float slice_height = slice.bottom - slice.top;
                        if (slice_width < kMinTileDim || slice_height < kMinTileDim) {
                            continue;
                        }
                        if (slice.end - slice.begin == 1) {
                            queue.push_back(QueueItem{
                                children[slice.begin],
                                slice.left,
                                slice.top,
                                slice.right,
                                slice.bottom,
                                item.depth + 1,
                            });
                            continue;
                        }

                        const double total = (std::max)(1.0, slice_weight(slice.begin, slice.end));
                        double prefix = 0.0;
                        size_t split = slice.begin + 1;
                        for (; split + 1 < slice.end; ++split) {
                            prefix += static_cast<double>(m_tree_nodes[children[split - 1]].physical_bytes);
                            if (prefix >= total * 0.5) {
                                break;
                            }
                        }
                        const double leading = slice_weight(slice.begin, split);
                        const float ratio = static_cast<float>(leading / total);
                        if (slice_width >= slice_height) {
                            const float split_x = std::clamp(
                                slice.left + (slice_width * ratio),
                                slice.left + 1.0f,
                                slice.right - 1.0f);
                            slices.push_back(Slice{ split, slice.end, split_x, slice.top, slice.right, slice.bottom });
                            slices.push_back(Slice{ slice.begin, split, slice.left, slice.top, split_x, slice.bottom });
                        } else {
                            const float split_y = std::clamp(
                                slice.top + (slice_height * ratio),
                                slice.top + 1.0f,
                                slice.bottom - 1.0f);
                            slices.push_back(Slice{ split, slice.end, slice.left, split_y, slice.right, slice.bottom });
                            slices.push_back(Slice{ slice.begin, split, slice.left, slice.top, slice.right, split_y });
                        }
                    }
                }
            } else {
                // No tree loaded yet (live scan in progress or empty index):
                // fall back to the flat catalog sample.
                layout_name = L"balanced";
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

                const float gap = 4.0f;
                tiles.reserve(tile_inputs.size());
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
            }

            // One brush recolored per tile: creating a COM brush object per
            // tile stalls the UI thread for seconds at 20k tiles.
            winrt::com_ptr<ID2D1SolidColorBrush> tile_brush;
            result = d2d_context->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Black), tile_brush.put());
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame D2D brush failed: " + HresultText(result);
                return;
            }
            for (auto const& tile : tiles) {
                tile_brush->SetColor(tile.color);
                const auto rect = D2D1::RectF(tile.left, tile.top, tile.right, tile.bottom);
                if (tile.frame) {
                    d2d_context->DrawRectangle(rect, tile_brush.get(), 1.0f);
                } else {
                    d2d_context->FillRectangle(rect, tile_brush.get());
                }
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
                ResetTreemapRenderStack();
                return;
            }

            result = swap_chain->Present(1, 0);
            if (FAILED(result)) {
                m_treemap_render_status = L"Treemap probe frame present failed: " + HresultText(result);
                ResetTreemapRenderStack();
                return;
            }
            // The swapchain is bound to the panel once, when it is (re)created
            // in EnsureTreemapRenderStack — no per-frame rebind.

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
                std::to_wstring(tiles.size()) + L" tiles, D3D feature level " +
                std::to_wstring(level_major) + L"." + std::to_wstring(level_minor) +
                L"; layout=" + layout_name +
                L", directories=" + std::to_wstring(directories_recursed) +
                L", labels=" + std::to_wstring(labels_drawn) +
                L", first tile=\"" + first_tile + L"\".";

            if (needs_deepening) {
                // The per-frame child-fetch budget ran out; refine the
                // treemap over the next coalesced render tick.
                m_treemap_render_dirty = true;
                ScheduleTreemapRender(L"progressive deepening");
            }
        }
        catch (winrt::hresult_error const& error) {
            m_treemap_render_status = L"Treemap probe frame failed: " + std::wstring(error.message().c_str());
            // An exception mid-draw can leave the cached D2D context in a
            // begun-draw state; drop the stack so the next render rebuilds.
            ResetTreemapRenderStack();
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
        // Reverse scan: children are laid out after their ancestors'
        // directory frames, so the innermost tile under the cursor wins.
        for (auto it = m_treemap_tile_layout.rbegin(); it != m_treemap_tile_layout.rend(); ++it) {
            auto const& tile = *it;
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
        // Reverse scan so the innermost tile under the tap wins (children
        // are laid out after their ancestors' frames).
        for (auto it = m_treemap_tile_layout.rbegin(); it != m_treemap_tile_layout.rend(); ++it) {
            auto const& tile = *it;
            if (point.X >= tile.left && point.X <= tile.right &&
                point.Y >= tile.top && point.Y <= tile.bottom) {
                SelectVisualizationTarget(tile.name, tile.path, tile.kind, tile.size_text);
                UpdateEventText(L"Selected GPU tile: " + tile.name);
                return;
            }
        }
    }

    void MainWindow::SetSection(ShellSection section)
    {
        TraceStartup(L"SetSection begin");
        m_active_section = section;

        // Sections no longer switch pages — the panes are one permanent
        // WinDirStat-style layout. Selecting Search/Diagnostics (keyboard
        // shortcut or reveal button) just makes that panel visible.
        if (section == ShellSection::Search) {
            m_show_search = true;
        }
        if (section == ShellSection::Diagnostics) {
            m_show_runtime_metrics = true;
            if (m_runtime_metrics_toggle) {
                m_runtime_metrics_toggle.IsChecked(true);
            }
        }
        UpdateSummaryText();

        if (m_session_active) {
            TraceStartup(L"SetSection visibility deferred during active scan");
            TraceStartup(L"SetSection end");
            return;
        }

        ApplyViewVisibility();
        TraceStartup(L"SetSection end");
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

    // Card titles follow the Obsidian Flux label style: small uppercase
    // monospace with wide tracking (the "ROOT DIRECTORIES" look from the
    // Storage Pulse mockups).
    Microsoft::UI::Xaml::Controls::TextBlock MainWindow::MakeCardTitle(std::wstring_view text) const
    {
        auto const& theme = ActiveShellTheme();
        std::wstring upper(text);
        for (auto& ch : upper) {
            ch = static_cast<wchar_t>(std::towupper(ch));
        }
        auto title = Microsoft::UI::Xaml::Controls::TextBlock{};
        title.Text(winrt::hstring(upper));
        title.FontSize(11.0);
        title.FontWeight({ 700 });
        title.CharacterSpacing(180);
        title.FontFamily(Microsoft::UI::Xaml::Media::FontFamily(L"Cascadia Mono, Consolas"));
        title.Foreground(MakeBrush(theme.text_secondary));
        return title;
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
                if (!TreemapSurface()) {
                    return;
                }

                // While a scan streams events, cap treemap redraws: catalog
                // flushes mark the map dirty continuously, and re-laying-out
                // the live arena tens of times per second starves the UI
                // thread.
                if (m_session_active) {
                    const auto now = std::chrono::steady_clock::now();
                    if (now - m_last_treemap_render_completed_at < std::chrono::milliseconds(750)) {
                        m_treemap_render_requested = true;
                        m_treemap_render_timer.Stop();
                        m_treemap_render_timer.Start();
                        return;
                    }
                }
                ++m_total_treemap_render_flush_count;

                const int width = (std::max)(1, static_cast<int>(TreemapSurface().ActualWidth()));
                const int height = (std::max)(1, static_cast<int>(TreemapSurface().ActualHeight()));
                RenderTreemapProbeFrame(width, height);
                m_last_treemap_render_completed_at = std::chrono::steady_clock::now();
                if (TreemapSurfaceStatusText()) {
                    TreemapSurfaceStatusText().Text(winrt::hstring(
                        L"SwapChainPanel host active: " +
                        std::to_wstring(width) + L"x" +
                        std::to_wstring(height) +
                        L" px. " + m_treemap_render_status));
                }
                {
                    std::lock_guard guard(m_pending_ui_mutex);
                    m_last_composition_frame_time = std::chrono::steady_clock::now();
                }
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
                m_pending_ui_state.visualization_dirty || m_pending_ui_state.catalog_dirty ||
                m_pending_ui_state.extension_stats_dirty ||
                m_pending_ui_state.shell_state_dirty ||
                !m_pending_ui_state.live_directories.empty();

            if (has_pending) {
                pending = std::move(m_pending_ui_state);
                m_pending_ui_state = {};
            }

            m_ui_flush_requested = false;
            m_ui_flush_timer.Stop();
        }

        if (!has_pending) {
            if (m_live_backlog_cursor < m_live_directory_backlog.size()) {
                // No fresh events, but queued live directories remain.
                ApplyLiveDirectories({});
            }
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
                m_fast_scan_unavailable = false;
                m_fast_scan_unavailable_message.clear();
            }
            if (pending.fast_scan_unavailable) {
                m_fast_scan_unavailable = true;
                m_fast_scan_unavailable_message = pending.fast_scan_unavailable_message;
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

        if (!pending.live_directories.empty()) {
            ApplyLiveDirectories(std::move(pending.live_directories));
        }

        if (pending.catalog_dirty && !pending.catalog_entries.empty()) {
            for (auto const& entry : pending.catalog_entries) {
                if (m_tree_catalog_keys.insert(TreeCatalogKey(entry)).second) {
                    m_tree_catalog.push_back(entry);
                }
            }
        }

        if (pending.shell_state_dirty) {
            // The scan session ended off the UI thread; hide the scanning
            // banner and refresh the status strip now that state settled.
            ApplyShellState();
        }

        if (pending.reload_snapshot) {
            if (m_session_active) {
                TraceStartup(L"FlushPendingUiState snapshot reload deferred during active scan");
            } else {
                LoadPersistedCatalogSnapshot();
            }
        } else if (pending.catalog_dirty && !pending.catalog_entries.empty()) {
            // The flat catalog previews are expensive to rebuild and not the
            // focus during a scan (the live tree owns the pane); they
            // refresh from the post-scan snapshot reload instead.
            if (!m_session_active) {
                UpdateTreeSnapshotPreview(FilterTreeCatalog());
                UpdateCatalogSnapshot();
            }
            m_treemap_render_dirty = true;
            ScheduleTreemapRender(L"catalog flush");
        }

        if (pending.extension_stats_dirty) {
            m_extension_stats = std::move(pending.extension_stats);
            PopulateExtensionList(m_extension_stats);
        }

        m_last_ui_flush_time = flush_started;
        m_last_working_set_bytes = CurrentWorkingSetBytes();
        m_peak_working_set_bytes = (std::max)(m_peak_working_set_bytes, m_last_working_set_bytes);
        {
            std::lock_guard guard(m_pending_ui_mutex);
            ++m_total_ui_flush_count;
            if (m_last_ui_event_time.time_since_epoch().count() > 0) {
                const auto latency = std::chrono::duration<double, std::milli>(flush_started - m_last_ui_event_time);
                m_last_ui_latency_ms = latency.count();
            }
            m_last_ui_flush_duration_ms = std::chrono::duration<double, std::milli>(
                std::chrono::steady_clock::now() - flush_started).count();
            m_peak_ui_flush_duration_ms = (std::max)(m_peak_ui_flush_duration_ms, m_last_ui_flush_duration_ms);
            m_last_composition_frame_time = std::chrono::steady_clock::now();
        }

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

        uint64_t total_ui_flush_count = 0;
        uint64_t pending_event_count = 0;
        double last_ui_latency_ms = 0.0;
        double last_input_latency_ms = 0.0;
        double last_ui_flush_duration_ms = 0.0;
        double peak_ui_flush_duration_ms = 0.0;
        uint64_t total_composition_frame_count = 0;
        double last_composition_frame_ms = 0.0;
        double peak_composition_frame_ms = 0.0;
        double elapsed_seconds = 0.0;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            if (m_scan_started_at.time_since_epoch().count() > 0) {
                elapsed_seconds = std::chrono::duration<double>(
                    std::chrono::steady_clock::now() - m_scan_started_at).count();
            }
            total_ui_flush_count = m_total_ui_flush_count;
            pending_event_count = m_pending_event_count;
            last_ui_latency_ms = m_last_ui_latency_ms;
            last_input_latency_ms = m_last_input_latency_ms;
            last_ui_flush_duration_ms = m_last_ui_flush_duration_ms;
            peak_ui_flush_duration_ms = m_peak_ui_flush_duration_ms;
            total_composition_frame_count = m_total_composition_frame_count;
            last_composition_frame_ms = m_last_composition_frame_ms;
            peak_composition_frame_ms = m_peak_composition_frame_ms;
        }
        const double items_per_second = elapsed_seconds > 0.0
            ? static_cast<double>(m_last_progress_items_done) / elapsed_seconds
            : 0.0;
        const double bytes_per_second = elapsed_seconds > 0.0
            ? static_cast<double>(m_last_progress_bytes_done) / elapsed_seconds
            : 0.0;

        const std::wstring text =
            L"UI batching: " + reason +
            L", flushes=" + std::to_wstring(total_ui_flush_count) +
            L", queued events=" + std::to_wstring(pending_event_count) +
            L", last latency=" + std::to_wstring(static_cast<int>(last_ui_latency_ms)) + L" ms" +
            L", last input=" + std::to_wstring(static_cast<int>(last_input_latency_ms)) + L" ms" +
            L", flush cost=" + std::to_wstring(static_cast<int>(last_ui_flush_duration_ms)) + L" ms" +
            L", peak flush=" + std::to_wstring(static_cast<int>(peak_ui_flush_duration_ms)) + L" ms" +
            L", frames=" + std::to_wstring(total_composition_frame_count) +
            L", last frame=" + std::to_wstring(static_cast<int>(last_composition_frame_ms)) + L" ms" +
            L", peak frame=" + std::to_wstring(static_cast<int>(peak_composition_frame_ms)) + L" ms" +
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

        std::wstring scan_duration_text;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            scan_duration_text = m_last_scan_duration_text;
        }

        const std::wstring text =
            L"Root path: " + m_current_root_path +
            L" | Section: " + SectionName(m_active_section) +
            L" | Selection: " + m_current_selection_name +
            L" (" + m_current_selection_kind + L")" +
            L" | Mode: " + std::wstring(IsProcessElevated() ? L"administrator" : L"standard") +
            L" | State: " + std::wstring(m_session_active ? L"scanning" : L"idle") +
            L" | Results: " + std::wstring(m_has_results ? L"loaded" : L"none") +
            L" | " + scan_duration_text +
            L" | Error: " + std::wstring(m_has_error ? L"yes" : L"no");

        SummaryText().Text(winrt::hstring(text));
    }

    void MainWindow::UpdateRuntimeSnapshot()
    {
        if (!RuntimeSnapshotText()) {
            return;
        }

        uint64_t total_ui_flush_count = 0;
        uint64_t pending_event_count = 0;
        double last_ui_latency_ms = 0.0;
        double last_input_latency_ms = 0.0;
        double last_ui_flush_duration_ms = 0.0;
        double peak_ui_flush_duration_ms = 0.0;
        uint64_t total_composition_frame_count = 0;
        double last_composition_frame_ms = 0.0;
        double peak_composition_frame_ms = 0.0;
        std::wstring scan_duration_text;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            total_ui_flush_count = m_total_ui_flush_count;
            pending_event_count = m_pending_event_count;
            last_ui_latency_ms = m_last_ui_latency_ms;
            last_input_latency_ms = m_last_input_latency_ms;
            last_ui_flush_duration_ms = m_last_ui_flush_duration_ms;
            peak_ui_flush_duration_ms = m_peak_ui_flush_duration_ms;
            total_composition_frame_count = m_total_composition_frame_count;
            last_composition_frame_ms = m_last_composition_frame_ms;
            peak_composition_frame_ms = m_peak_composition_frame_ms;
            scan_duration_text = m_last_scan_duration_text;
        }

        const std::wstring text =
            L"UI batching: " + std::wstring(m_shell_ready ? L"ready" : L"starting") +
            L", flushes=" + std::to_wstring(total_ui_flush_count) +
            L", queued events=" + std::to_wstring(pending_event_count) +
            L", last latency=" + std::to_wstring(static_cast<int>(last_ui_latency_ms)) + L" ms" +
            L", last input=" + std::to_wstring(static_cast<int>(last_input_latency_ms)) + L" ms" +
            L", flush cost=" + std::to_wstring(static_cast<int>(last_ui_flush_duration_ms)) + L" ms" +
            L", peak flush=" + std::to_wstring(static_cast<int>(peak_ui_flush_duration_ms)) + L" ms" +
            L", frames=" + std::to_wstring(total_composition_frame_count) +
            L", last frame=" + std::to_wstring(static_cast<int>(last_composition_frame_ms)) + L" ms" +
            L", peak frame=" + std::to_wstring(static_cast<int>(peak_composition_frame_ms)) + L" ms" +
            L", treemap renders=" + std::to_wstring(m_total_treemap_render_flush_count) +
            L"/" + std::to_wstring(m_total_treemap_render_request_count) +
            L" requests, coalesced=" + std::to_wstring(m_total_treemap_render_coalesced_count) +
            L", session=" + std::wstring(m_session_active ? L"active" : L"idle") +
            L", results=" + std::wstring(m_has_results ? L"loaded" : L"none") +
            L", working set=" + FormatBytes(m_last_working_set_bytes) +
            L", peak=" + FormatBytes(m_peak_working_set_bytes) +
            L", " + m_last_cache_load_text +
            L", " + scan_duration_text;

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

        std::wstring text;
        if (m_fast_scan_unavailable) {
            text += L"SLOW SCAN MODE: the fast NTFS scan is unavailable, so this scan is "
                L"running the much slower standard directory walk. Restart WinBlaze as "
                L"Administrator for full-speed scans. (" + m_fast_scan_unavailable_message + L") | ";
        }
        text +=
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

    // Mirrors the native bridge's extension_key(): lowercase extension, or
    // empty for dotfiles and names without one. The treemap and legend must
    // derive identical keys or their colors drift apart.
    std::wstring MainWindow::ExtensionKeyFromName(std::wstring const& name)
    {
        const auto dot = name.rfind(L'.');
        if (dot == std::wstring::npos || dot == 0 || dot + 1 >= name.size()) {
            return {};
        }
        std::wstring extension = name.substr(dot + 1);
        for (auto& ch : extension) {
            if (ch >= L'A' && ch <= L'Z') {
                ch = static_cast<wchar_t>(ch - L'A' + L'a');
            }
        }
        return extension;
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
        catalog_entry.allocation_bytes = entry.allocation_bytes;
        catalog_entry.total_entries = entry.total_entries;
        catalog_entry.description = to_wide(entry.description);
        if (entry.has_modified_utc) {
            catalog_entry.modified_utc = static_cast<int64_t>(entry.modified_utc);
        }
        // Percentage is relative to the running scan-root total rather than
        // the entry's immediate parent: parent directory rollups are only
        // finalized once a full scan/aggregation pass completes, so a
        // parent-relative percentage would sit at 0% for most of a live
        // scan. The scan-root total updates continuously via Progress/
        // Summary events, so this stays meaningful throughout.
        if (catalog_entry.kind == L"Volume") {
            catalog_entry.progress = 100;
        } else if (m_last_summary_total_size_bytes > 0) {
            const double ratio =
                static_cast<double>(entry.size_bytes) /
                static_cast<double>(m_last_summary_total_size_bytes);
            catalog_entry.progress = std::clamp(static_cast<int>(ratio * 100.0), 0, 100);
        } else {
            catalog_entry.progress = 0;
        }
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
        {
            std::lock_guard guard(m_pending_ui_mutex);
            text += L" | " + m_last_scan_duration_text;
        }

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

    // Refreshes every control that mirrors the current selection: the detail
    // card labels and the status-bar summary. (The breadcrumb trail this
    // method used to build was removed with the header bar.)
    void MainWindow::UpdateBreadcrumbs()
    {
        if (SelectionText() && SelectionSizeText()) {
            SelectionText().Text(winrt::hstring(
                L"Selection: " + m_current_selection_name + L" (" + m_current_selection_kind + L")"));
            SelectionSizeText().Text(winrt::hstring(L"Size: " + m_current_selection_size));
        }

        if (m_selection_status_text) {
            m_selection_status_text.Text(winrt::hstring(
                m_current_selection_name + L" (" + m_current_selection_kind + L") \u00B7 " +
                m_current_selection_size));
        }
    }

    void MainWindow::UpdateStatus(std::wstring const& text)
    {
        if (!StatusText()) {
            return;
        }
        StatusText().Text(winrt::hstring(text));
        if (m_sidebar_status_text) {
            m_sidebar_status_text.Text(winrt::hstring(L"ENGINE: " + text));
        }
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

        // Fetch off the UI thread: the first call after startup loads and
        // decodes the persisted snapshot and builds the tree index, which is
        // multi-second work on a full-drive catalog. The generation counter
        // discards results superseded by a newer scan/reload. The window
        // (and therefore `this`) outlives the app's dispatcher queue, which
        // stops running callbacks at shutdown.
        const uint64_t generation = ++m_snapshot_load_generation;
        UpdateStatus(L"Loading catalog snapshot...");
        auto dispatcher = DispatcherQueue();
        std::thread([this, generation, dispatcher]() {
            auto snapshot = std::make_shared<std::vector<TreeCatalogEntry>>();
            WbIndexSnapshotStats stats{};
            stats = ::WinBlaze::UI::NativeBridge::LoadCatalogSnapshotWithStats(
                [&snapshot, this](WbCatalogEntry const& entry) {
                    if (snapshot->size() < kCatalogSnapshotLoadLimit) {
                        snapshot->push_back(CatalogEntryFromNative(entry));
                    }
                });
            dispatcher.TryEnqueue([this, generation, stats, snapshot]() {
                if (generation != m_snapshot_load_generation.load()) {
                    TraceStartup(L"LoadPersistedCatalogSnapshot stale result dropped");
                    return;
                }
                ApplyPersistedCatalogSnapshot(stats, std::move(*snapshot));
            });
        }).detach();
    }

    void MainWindow::ApplyPersistedCatalogSnapshot(
        WbIndexSnapshotStats stats,
        std::vector<TreeCatalogEntry> snapshot)
    {
        TraceStartup(L"ApplyPersistedCatalogSnapshot begin");
        m_tree_catalog.clear();
        m_tree_catalog_keys.clear();
        m_instant_search_hits.clear();
        m_tree_window_offset = 0;

        if (stats.files + stats.directories > 0) {
            std::lock_guard guard(m_pending_ui_mutex);
            m_progress_total_estimate = stats.files + stats.directories;
        }

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
            TraceStartup(L"ApplyPersistedCatalogSnapshot empty");
            UpdateTreeSnapshotPreview(std::vector<TreeCatalogEntry>{});
            UpdateSearchResultsPreview(std::vector<TreeCatalogEntry>{});
            UpdateCatalogSnapshot();
            m_treemap_render_dirty = true;
            ScheduleTreemapRender(L"empty snapshot");
            UpdateSummaryText();
            UpdateRuntimeSnapshot();
            PopulateExtensionList(std::vector<ExtensionStatEntry>{});
            UpdateStatus(L"No catalog snapshot available.");
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
        // Replace the flat preview in the tree pane with the real expandable
        // folder tree served by the native tree index (hot cache by now).
        LoadTreeSnapshot();
        // Extension stats spawn another FFI pass plus a ListView rebuild;
        // run them on the next dispatcher tick so this apply never blocks
        // the UI thread for one long stretch.
        DispatcherQueue().TryEnqueue([this]() {
            LoadExtensionStatsSnapshot();
        });
        UpdateStatus(L"Catalog snapshot loaded.");
        TraceStartup(L"ApplyPersistedCatalogSnapshot end");
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

    std::wstring MainWindow::ScanElapsedText()
    {
        std::chrono::steady_clock::time_point started;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            started = m_scan_started_at;
        }
        if (started.time_since_epoch().count() <= 0) {
            return L"";
        }
        const auto total_seconds = std::chrono::duration_cast<std::chrono::seconds>(
            std::chrono::steady_clock::now() - started).count();
        if (total_seconds < 60) {
            return std::to_wstring(total_seconds) + L"s";
        }
        const auto seconds = total_seconds % 60;
        return std::to_wstring(total_seconds / 60) + L"m " +
            (seconds < 10 ? L"0" : L"") + std::to_wstring(seconds) + L"s";
    }

    void MainWindow::HandleNativeEvent(WbEvent const& event)
    {
        std::wstring status_text;
        std::wstring event_text;
        std::wstring summary_text;
        std::wstring progress_text;
        std::wstring error_text;
        std::wstring last_issue_text;
        std::vector<ExtensionStatEntry> extension_stats_parsed;
        double progress_percent = 0.0;
        bool clear_session = false;
        bool mark_has_results = false;
        bool reload_snapshot = false;
        bool reset_scan_timing = false;
        bool update_scan_duration = false;

        switch (event.kind) {
        case WbEventKind_SessionStarted:
            TraceStartup(L"HandleNativeEvent session started");
            status_text = L"Scanning...";
            event_text = L"Session started.";
            progress_text = L"0% complete";
            progress_percent = 0.0;
            reset_scan_timing = true;
            mark_has_results = true;
            break;
        case WbEventKind_Progress:
        {
            TraceStartup(
                L"HandleNativeEvent progress " +
                std::to_wstring(event.progress_items_done) + L"/" +
                std::to_wstring(event.progress_items_total));
            status_text = L"Scanning...";
            const std::wstring elapsed = ScanElapsedText();
            const std::wstring elapsed_suffix =
                elapsed.empty() ? std::wstring() : (L" \u00B7 " + elapsed);
            if (event.progress_items_total == 0) {
                // Directory-walk backend: the total is unknowable up front,
                // so estimate against the previous completed scan's item
                // count (capped below 100%) instead of pinning the bar at 0
                // until the summary snaps it to 100.
                uint64_t total_estimate = 0;
                {
                    std::lock_guard guard(m_pending_ui_mutex);
                    total_estimate = m_progress_total_estimate;
                }
                if (total_estimate > 0) {
                    progress_percent = (std::min)(
                        99.0,
                        (static_cast<double>(event.progress_items_done) /
                            static_cast<double>(total_estimate)) * 100.0);
                    progress_text = L"~" + std::to_wstring(static_cast<int>(progress_percent)) +
                        L"% complete" + elapsed_suffix;
                } else {
                    // First-ever scan: no estimate available; creep the bar
                    // asymptotically so it visibly moves while staying honest
                    // about not knowing the total.
                    const double done = static_cast<double>(event.progress_items_done);
                    progress_percent = (std::min)(99.0, (done / (done + 500000.0)) * 100.0);
                    progress_text = L"Items discovered: " +
                        std::to_wstring(event.progress_items_done) + elapsed_suffix;
                }
                event_text = L"Scanning in progress: " +
                    std::to_wstring(event.progress_items_done) + L" items discovered";
            } else {
                progress_percent = (static_cast<double>(event.progress_items_done) /
                    static_cast<double>(event.progress_items_total)) * 100.0;
                event_text = L"Scanning in progress: " +
                    std::to_wstring(event.progress_items_done) + L"/" +
                    std::to_wstring(event.progress_items_total) + L" items";
                progress_text = std::to_wstring(static_cast<int>(progress_percent)) +
                    L"% complete" + elapsed_suffix;
            }
            mark_has_results = true;
            break;
        }
        case WbEventKind_Summary:
        {
            TraceStartup(L"HandleNativeEvent summary");
            status_text = L"Finalizing...";
            event_text = FormatSummary(event);
            summary_text = event_text;
            progress_percent = 100.0;
            const std::wstring elapsed = ScanElapsedText();
            progress_text = L"100% complete" +
                (elapsed.empty() ? std::wstring() : (L" \u00B7 " + elapsed));
            update_scan_duration = true;
            mark_has_results = true;
            break;
        }
        case WbEventKind_Completed:
        {
            TraceStartup(L"HandleNativeEvent completed");
            status_text = L"Completed.";
            event_text = L"Scan completed.";
            progress_percent = 100.0;
            const std::wstring elapsed = ScanElapsedText();
            progress_text = L"100% complete" +
                (elapsed.empty() ? std::wstring() : (L" \u00B7 " + elapsed));
            update_scan_duration = true;
            clear_session = true;
            reload_snapshot = true;
            mark_has_results = true;
            break;
        }
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
        case WbEventKind_ExtensionStats:
            TraceStartup(L"HandleNativeEvent extension stats");
            extension_stats_parsed.reserve(event.extension_stats.count);
            for (size_t index = 0; index < event.extension_stats.count; ++index) {
                extension_stats_parsed.push_back(
                    ExtensionStatFromNative(event.extension_stats.items[index]));
            }
            break;
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
            // Directories now arrive batched (WbEventKind_DirectoryBatch);
            // an individual event only marks that results exist.
            mark_has_results = true;
            break;
        case WbEventKind_DirectoryBatch:
            // Queued for the live folder tree in the lock block below.
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

            // m_scan_started_at / m_last_scan_duration_text / the UI timing
            // counters below are also read from the UI thread (FlushPendingUiState,
            // UpdatePerformanceCounters, UpdateSummaryText, UpdateRuntimeSnapshot,
            // OnCompositionRendering) while this function runs on the native
            // scan callback thread. Mutating them here, under the same mutex
            // those readers take, avoids the unsynchronized cross-thread
            // std::wstring/time_point access that could tear a read and crash.
            if (reset_scan_timing) {
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
            } else if (update_scan_duration && m_scan_started_at.time_since_epoch().count() > 0) {
                const auto elapsed = std::chrono::steady_clock::now() - m_scan_started_at;
                m_last_scan_duration_text = L"Scan duration: " +
                    std::to_wstring(static_cast<int>(std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count())) +
                    L" ms";
            }

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
                m_pending_ui_state.extension_stats_dirty = true;
                m_pending_ui_state.extension_stats.clear();
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
                m_progress_total_estimate =
                    event.summary.files_seen + event.summary.directories_seen;
            } else if (event.kind == WbEventKind_Issue) {
                m_pending_ui_state.correctness_dirty = true;
                ++m_pending_ui_state.issue_count_delta;
                ++m_pending_ui_state.issue_code_deltas[static_cast<uint32_t>(event.error.code)];
                m_pending_ui_state.last_issue_text = last_issue_text;
                m_pending_ui_state.recent_issue_texts.push_back(last_issue_text);
                if (static_cast<uint32_t>(event.error.code) == kFastScanUnavailableIssueCode) {
                    m_pending_ui_state.fast_scan_unavailable = true;
                    m_pending_ui_state.fast_scan_unavailable_message = last_issue_text;
                }
            } else if (event.kind == WbEventKind_IncrementalChanges) {
                m_pending_ui_state.correctness_dirty = true;
                m_pending_ui_state.incremental_changes_dirty = true;
                m_pending_ui_state.incremental_added = event.incremental_changes.added;
                m_pending_ui_state.incremental_removed = event.incremental_changes.removed;
                m_pending_ui_state.incremental_modified = event.incremental_changes.modified;
                m_pending_ui_state.incremental_renamed = event.incremental_changes.renamed;
                m_pending_ui_state.incremental_moved = event.incremental_changes.moved;
            } else if (event.kind == WbEventKind_ExtensionStats) {
                m_pending_ui_state.extension_stats_dirty = true;
                m_pending_ui_state.extension_stats = std::move(extension_stats_parsed);
            }

            if (event.kind == WbEventKind_DirectoryBatch) {
                // Live folder tree: queue the whole batch; FlushPendingUiState
                // splices it into the arena. Directories deliberately skip
                // the flat catalog (m_tree_catalog) — the full set would
                // duplicate hundreds of MB of strings. Names stay UTF-8 here;
                // the UI thread converts them at apply time.
                auto const& batch = event.directory_batch;
                if (batch.items != nullptr && batch.count > 0) {
                    auto& queued = m_pending_ui_state.live_directories;
                    queued.reserve(queued.size() + batch.count);
                    for (size_t index = 0; index < batch.count; ++index) {
                        auto const& item = batch.items[index];
                        queued.push_back(LiveDirectory{
                            item.id,
                            item.parent_id,
                            item.has_parent != 0,
                            item.name.ptr == nullptr
                                ? std::string{}
                                : std::string(item.name.ptr, item.name.len),
                        });
                    }
                }
            } else if (event.kind == WbEventKind_VolumeDiscovered ||
                event.kind == WbEventKind_FileFound ||
                event.kind == WbEventKind_SessionStarted) {
                m_pending_ui_state.catalog_dirty = true;
                if (event.kind != WbEventKind_FileFound || m_pending_ui_state.catalog_entries.size() < 256) {
                    m_pending_ui_state.catalog_entries.push_back(CatalogEntryFromNative(event.catalog_entry));
                }
            }

            // OR rather than overwrite: a Completed event's reload_snapshot=true
            // must survive even if another event (e.g. a force-emitted
            // ExtensionStats snapshot) is batched into the same UI flush
            // right after it and would otherwise reset this back to false.
            m_pending_ui_state.reload_snapshot = m_pending_ui_state.reload_snapshot || reload_snapshot;

            if (clear_session) {
                auto session = m_session;
                m_session = {};
                m_session_active = false;
                m_pending_ui_state.shell_state_dirty = true;
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

        auto percentage_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        percentage_text.Text(winrt::hstring(std::to_wstring(std::clamp(entry.progress, 0, 100)) + L"%"));
        percentage_text.MinWidth(44.0);
        percentage_text.Opacity(0.8);
        percentage_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(percentage_text);

        auto size_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        size_text.Text(winrt::hstring(entry.size_text));
        size_text.MinWidth(72.0);
        size_text.Opacity(0.8);
        size_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(size_text);

        auto physical_size_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        physical_size_text.Text(winrt::hstring(FormatBytes(entry.allocation_bytes)));
        physical_size_text.MinWidth(72.0);
        physical_size_text.Opacity(0.8);
        physical_size_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(physical_size_text);

        auto items_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        items_text.Text(winrt::hstring(
            entry.kind == L"File" ? std::wstring(L"-") : std::to_wstring(entry.total_entries)));
        items_text.MinWidth(56.0);
        items_text.Opacity(0.72);
        items_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(items_text);

        auto last_change_text = Microsoft::UI::Xaml::Controls::TextBlock{};
        last_change_text.Text(winrt::hstring(
            entry.modified_utc.has_value() ? FormatFileTimeUtc(entry.modified_utc.value()) : L"-"));
        last_change_text.MinWidth(120.0);
        last_change_text.Opacity(0.72);
        last_change_text.VerticalAlignment(Microsoft::UI::Xaml::VerticalAlignment::Center);
        row.Children().Append(last_change_text);

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
        // Once the real folder tree is loaded, flat catalog refreshes (live
        // scan previews, search filters) must not clobber it; search results
        // render in the search card instead.
        if (TreeArenaActive()) {
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

    void MainWindow::LoadTreeSnapshot()
    {
        TraceStartup(L"LoadTreeSnapshot begin");
        m_tree_nodes.clear();
        m_tree_visible_rows.clear();
        m_tree_node_index_by_id.clear();
        m_live_orphans.clear();
        m_live_directory_backlog.clear();
        m_live_backlog_cursor = 0;
        m_tree_window_offset = 0;

        ::WinBlaze::UI::NativeBridge::Initialize();
        const bool has_root = ::WinBlaze::UI::NativeBridge::TreeRoot([this](WbTreeNode const& node) {
            TreeNodeUi root;
            root.id = node.id;
            root.is_directory = node.is_directory != 0;
            root.name = node.name.ptr == nullptr
                ? std::wstring{}
                : Utf8ToWide(std::string_view{ node.name.ptr, node.name.len });
            root.logical_bytes = node.logical_bytes;
            root.physical_bytes = node.physical_bytes;
            root.file_count = node.file_count;
            root.item_count = node.item_count;
            root.modified_utc = node.modified_utc;
            root.has_modified_utc = node.has_modified_utc != 0;
            m_tree_nodes.push_back(std::move(root));
        });

        if (!has_root || m_tree_nodes.empty()) {
            m_tree_nodes.clear();
            TraceStartup(L"LoadTreeSnapshot: no tree root available");
            return;
        }

        EnsureTreeChildrenLoaded(0);
        m_tree_nodes[0].expanded = true;
        RebuildTreeVisibleRows();
        RefreshTreeListView();
        TraceStartup(L"LoadTreeSnapshot end");
    }

    // Splices scan-discovered directories into the live folder tree. Parents
    // always precede children in walk order; MFT-streamed entries can arrive
    // out of order, in which case the orphan is skipped here and appears when
    // the finished tree replaces the live one. Sizes stay pending ("...")
    // until the scan completes and rollups exist.
    void MainWindow::ApplyLiveDirectories(std::vector<LiveDirectory> directories)
    {
        // Accumulate into the backlog and apply a bounded chunk per flush: a
        // fast scan can hand one flush tens of thousands of directories, and
        // splicing them all at once stalls the UI thread.
        constexpr size_t kMaxLiveDirectoriesPerFlush = 4096;
        // Live rows deeper than this are dropped: the live view exists to
        // show scan progress at the top of the hierarchy, and materializing
        // every deep directory burns UI-thread time for rows nobody can see
        // until they expand. The complete tree replaces this at scan end.
        constexpr uint32_t kMaxLiveDepth = 3;
        if (m_live_backlog_cursor >= m_live_directory_backlog.size()) {
            m_live_directory_backlog = std::move(directories);
            m_live_backlog_cursor = 0;
        } else if (!directories.empty()) {
            m_live_directory_backlog.insert(
                m_live_directory_backlog.end(),
                std::make_move_iterator(directories.begin()),
                std::make_move_iterator(directories.end()));
        }
        // Consume via cursor: erasing the processed prefix each flush moved
        // the whole several-hundred-thousand-element tail every 16ms.
        const size_t chunk_begin = m_live_backlog_cursor;
        const size_t chunk_end = (std::min)(
            chunk_begin + kMaxLiveDirectoriesPerFlush, m_live_directory_backlog.size());
        m_live_backlog_cursor = chunk_end;
        if (m_live_backlog_cursor >= m_live_directory_backlog.size()) {
            // Fully consumed; release the storage next assignment.
        } else {
            // Keep draining even if no further scan events arrive.
            ScheduleUiFlush();
        }

        bool structure_changed = false;

        // Insert one directory and drain any orphans that were waiting for
        // it, iteratively (orphan chains can be deep).
        auto insert_with_orphans = [&](LiveDirectory const& first, size_t first_parent_index) {
            std::vector<std::pair<LiveDirectory, size_t>> work;
            work.emplace_back(first, first_parent_index);
            while (!work.empty()) {
                auto [directory, parent_index] = std::move(work.back());
                work.pop_back();

                TreeNodeUi node;
                node.id = directory.id;
                node.is_directory = true;
                node.name = Utf8ToWide(directory.name_utf8);
                node.children_loaded = true;
                const size_t node_index = m_tree_nodes.size();
                if (parent_index == SIZE_MAX) {
                    node.expanded = true; // root
                } else {
                    node.depth = m_tree_nodes[parent_index].depth + 1;
                    node.parent = parent_index;
                }
                m_tree_node_index_by_id[directory.id] = node_index;
                m_tree_nodes.push_back(std::move(node));
                if (parent_index != SIZE_MAX) {
                    m_tree_nodes[parent_index].children.push_back(node_index);
                }
                structure_changed = true;

                const auto orphan_it = m_live_orphans.find(directory.id);
                if (orphan_it != m_live_orphans.end()) {
                    // Only drain orphans that stay within the live depth cap.
                    if (m_tree_nodes[node_index].depth + 1 <= kMaxLiveDepth) {
                        for (auto& orphan : orphan_it->second) {
                            work.emplace_back(std::move(orphan), node_index);
                        }
                    }
                    m_live_orphans.erase(orphan_it);
                }
            }
        };

        for (size_t backlog_index = chunk_begin; backlog_index < chunk_end; ++backlog_index) {
            auto& directory = m_live_directory_backlog[backlog_index];
            if (m_tree_node_index_by_id.count(directory.id) != 0) {
                continue;
            }

            if (!directory.has_parent) {
                if (!m_tree_nodes.empty()) {
                    continue;
                }
                insert_with_orphans(directory, SIZE_MAX);
                continue;
            }

            const auto parent_it = m_tree_node_index_by_id.find(directory.parent_id);
            if (parent_it == m_tree_node_index_by_id.end()) {
                // Parent not seen yet (per-worker event batching reorders
                // across workers) — or it was depth-capped. Park a bounded
                // number: reordering recovery only matters for the shallow
                // levels that arrive in the first moments of the scan.
                if (m_live_orphans.size() < 5000) {
                    m_live_orphans[directory.parent_id].push_back(std::move(directory));
                }
                continue;
            }
            if (m_tree_nodes[parent_it->second].depth + 1 > kMaxLiveDepth) {
                continue;
            }
            insert_with_orphans(directory, parent_it->second);
        }

        if (!structure_changed) {
            return;
        }

        // Directories stream in constantly during a scan, and rebuilding the
        // ListView costs real UI-thread time (hundreds of XAML objects); at
        // a 500ms cadence the dispatcher never idled and input/automation
        // starved for the scan's whole duration. Refresh sparsely — the live
        // view is a progress indicator, not a working surface.
        const auto now = std::chrono::steady_clock::now();
        if (now - m_last_live_tree_refresh < std::chrono::milliseconds(2000)) {
            return;
        }
        m_last_live_tree_refresh = now;
        RebuildTreeVisibleRows();
        RefreshTreeListView();
    }

    void MainWindow::EnsureTreeChildrenLoaded(size_t node_index)
    {
        if (node_index >= m_tree_nodes.size()) {
            return;
        }
        if (m_tree_nodes[node_index].children_loaded ||
            !m_tree_nodes[node_index].is_directory ||
            m_tree_nodes[node_index].is_more_row) {
            return;
        }

        const uint64_t parent_id = m_tree_nodes[node_index].id;
        const uint32_t child_depth = m_tree_nodes[node_index].depth + 1;
        std::vector<size_t> children;

        const auto result = ::WinBlaze::UI::NativeBridge::TreeChildren(
            parent_id,
            0,
            [this, node_index, child_depth, &children](WbTreeNode const& node) {
                TreeNodeUi child;
                child.id = node.id;
                child.is_directory = node.is_directory != 0;
                child.name = node.name.ptr == nullptr
                    ? std::wstring{}
                    : Utf8ToWide(std::string_view{ node.name.ptr, node.name.len });
                child.logical_bytes = node.logical_bytes;
                child.physical_bytes = node.physical_bytes;
                child.file_count = node.file_count;
                child.item_count = node.item_count;
                child.modified_utc = node.modified_utc;
                child.has_modified_utc = node.has_modified_utc != 0;
                child.depth = child_depth;
                child.parent = node_index;
                children.push_back(m_tree_nodes.size());
                m_tree_nodes.push_back(std::move(child));
            });

        if (result.total > result.emitted) {
            TreeNodeUi more;
            more.is_more_row = true;
            more.name = L"+ " + std::to_wstring(result.total - result.emitted) +
                L" more items (largest are shown)";
            more.depth = child_depth;
            more.parent = node_index;
            children.push_back(m_tree_nodes.size());
            m_tree_nodes.push_back(std::move(more));
        }

        m_tree_nodes[node_index].children = std::move(children);
        m_tree_nodes[node_index].children_loaded = true;
    }

    void MainWindow::RebuildTreeVisibleRows()
    {
        m_tree_visible_rows.clear();
        if (m_tree_nodes.empty()) {
            return;
        }

        std::vector<size_t> stack;
        stack.push_back(0);
        while (!stack.empty()) {
            const size_t index = stack.back();
            stack.pop_back();
            m_tree_visible_rows.push_back(index);
            auto const& node = m_tree_nodes[index];
            if (node.is_directory && node.expanded) {
                for (auto child = node.children.rbegin(); child != node.children.rend(); ++child) {
                    stack.push_back(*child);
                }
            }
        }
    }

    void MainWindow::ToggleTreeNodeExpansion(size_t node_index)
    {
        if (node_index >= m_tree_nodes.size()) {
            return;
        }
        if (!m_tree_nodes[node_index].is_directory || m_tree_nodes[node_index].is_more_row) {
            return;
        }

        // EnsureTreeChildrenLoaded grows m_tree_nodes (invalidating
        // references), so re-index rather than holding a reference.
        if (!m_tree_nodes[node_index].expanded) {
            EnsureTreeChildrenLoaded(node_index);
        }
        m_tree_nodes[node_index].expanded = !m_tree_nodes[node_index].expanded;
        RebuildTreeVisibleRows();
        RefreshTreeListView();
    }

    // Pages the next chunk of children into a directory whose "+N more" row
    // was activated: fetches from the native tree at the loaded offset,
    // splices before the more-row, and updates or retires the more-row.
    void MainWindow::LoadMoreTreeChildren(size_t more_index)
    {
        if (more_index >= m_tree_nodes.size() || !m_tree_nodes[more_index].is_more_row) {
            return;
        }
        const size_t parent_index = m_tree_nodes[more_index].parent;
        if (parent_index == SIZE_MAX || parent_index >= m_tree_nodes.size()) {
            return;
        }

        const uint64_t parent_id = m_tree_nodes[parent_index].id;
        const uint32_t child_depth = m_tree_nodes[parent_index].depth + 1;
        // Children before the trailing more-row are the already-loaded set.
        const size_t loaded = m_tree_nodes[parent_index].children.empty()
            ? 0
            : m_tree_nodes[parent_index].children.size() - 1;

        std::vector<size_t> fetched;
        const auto result = ::WinBlaze::UI::NativeBridge::TreeChildren(
            parent_id,
            static_cast<uint64_t>(loaded),
            [this, parent_index, child_depth, &fetched](WbTreeNode const& node) {
                TreeNodeUi child;
                child.id = node.id;
                child.is_directory = node.is_directory != 0;
                child.name = node.name.ptr == nullptr
                    ? std::wstring{}
                    : Utf8ToWide(std::string_view{ node.name.ptr, node.name.len });
                child.logical_bytes = node.logical_bytes;
                child.physical_bytes = node.physical_bytes;
                child.file_count = node.file_count;
                child.item_count = node.item_count;
                child.modified_utc = node.modified_utc;
                child.has_modified_utc = node.has_modified_utc != 0;
                child.depth = child_depth;
                child.parent = parent_index;
                fetched.push_back(m_tree_nodes.size());
                m_tree_nodes.push_back(std::move(child));
            });
        if (fetched.empty()) {
            return;
        }

        auto& siblings = m_tree_nodes[parent_index].children;
        siblings.pop_back(); // detach the more-row
        siblings.insert(siblings.end(), fetched.begin(), fetched.end());

        const uint64_t now_loaded = loaded + result.emitted;
        if (now_loaded < result.total) {
            m_tree_nodes[more_index].name =
                L"+ " + std::to_wstring(result.total - now_loaded) +
                L" more items (largest are shown)";
            siblings.push_back(more_index); // reattach at the end
        }

        RebuildTreeVisibleRows();
        RefreshTreeListView();
        UpdateStatus(L"Loaded " + std::to_wstring(result.emitted) + L" more rows.");
    }

    std::wstring MainWindow::TreeNodePath(size_t node_index) const
    {
        std::vector<size_t> chain;
        size_t current = node_index;
        while (current != SIZE_MAX && current < m_tree_nodes.size()) {
            chain.push_back(current);
            current = m_tree_nodes[current].parent;
        }

        std::wstring path;
        for (auto it = chain.rbegin(); it != chain.rend(); ++it) {
            auto const& segment = m_tree_nodes[*it].name;
            if (path.empty()) {
                path = segment;
            } else {
                if (!path.empty() && path.back() != L'\\') {
                    path += L'\\';
                }
                path += segment;
            }
        }
        return path;
    }

    void MainWindow::RefreshTreeListView()
    {
        if (!TreeListView() || !m_tree_updates_ready) {
            return;
        }

        auto const& rows = m_tree_visible_rows;
        if (rows.empty()) {
            m_tree_window_offset = 0;
        } else if (m_tree_window_offset >= rows.size()) {
            m_tree_window_offset =
                ((rows.size() - 1) / kTreeListVirtualizedWindowLimit) * kTreeListVirtualizedWindowLimit;
        }

        // During a scan the list is a lightweight progress view: render a
        // small window so each refresh stays cheap on the UI thread.
        const size_t window_limit = m_session_active ? 64 : kTreeListVirtualizedWindowLimit;
        const auto window_start = (std::min)(m_tree_window_offset, rows.size());
        const auto window_end = (std::min)(window_start + window_limit, rows.size());

        m_tree_selection_updates_suppressed = true;
        TreeListView().Items().Clear();
        for (size_t index = window_start; index < window_end; ++index) {
            TreeListView().Items().Append(CreateTreeNodeListItem(rows[index]));
        }
        m_tree_selection_updates_suppressed = false;

        if (TreeWindowPreviousButton()) {
            TreeWindowPreviousButton().IsEnabled(window_start > 0);
        }
        if (TreeWindowNextButton()) {
            TreeWindowNextButton().IsEnabled(window_end < rows.size());
        }

        if (TreeListStatusText()) {
            std::wstring status;
            if (rows.empty()) {
                status = L"Showing 0 of 0 folder tree rows with virtualized ListView containers.";
            } else {
                status = L"Showing rows " + std::to_wstring(window_start + 1) +
                    L"-" + std::to_wstring(window_end) +
                    L" of " + std::to_wstring(rows.size()) +
                    L" folder tree rows with virtualized ListView containers; expand folders to load more";
                if (rows.size() > kTreeListVirtualizedWindowLimit) {
                    status += L"; page size is " +
                        std::to_wstring(kTreeListVirtualizedWindowLimit) +
                        L" rows to keep redraws responsive";
                }
                status += L".";
            }
            TreeListStatusText().Text(winrt::hstring(status));
        }
    }

    Microsoft::UI::Xaml::Controls::ListViewItem MainWindow::CreateTreeNodeListItem(size_t node_index)
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;

        auto const& node = m_tree_nodes[node_index];

        auto item = ListViewItem{};
        item.Tag(box_value(static_cast<uint64_t>(node_index)));
        if (node.is_more_row) {
            // Single tap pages in the next chunk of this directory's
            // children. Deferred: the refresh replaces the ListView items.
            item.Tapped([this, node_index](auto const&, auto const&) {
                DispatcherQueue().TryEnqueue([this, node_index]() {
                    LoadMoreTreeChildren(node_index);
                });
            });
        }
        if (node.is_directory && !node.is_more_row) {
            // Tap a folder row to expand/collapse. Deferred through the
            // dispatcher: the refresh replaces the ListView items, and doing
            // that synchronously inside the tapped item's own event is
            // re-entrant.
            item.DoubleTapped([this, node_index](auto const&, auto const&) {
                DispatcherQueue().TryEnqueue([this, node_index]() {
                    ToggleTreeNodeExpansion(node_index);
                });
            });
        }

        auto row = StackPanel{};
        row.Orientation(Orientation::Horizontal);
        row.Spacing(12);

        // Indent + expander glyph + name.
        const double indent_width = (std::min)(240.0, static_cast<double>(node.depth) * 16.0);
        auto glyph = TextBlock{};
        glyph.Text(winrt::hstring(
            node.is_directory && !node.is_more_row ? (node.expanded ? L"\u25BE" : L"\u25B8") : L" "));
        glyph.Width(14.0);
        glyph.Margin(Thickness{ indent_width, 0.0, 0.0, 0.0 });
        glyph.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(glyph);

        auto label = TextBlock{};
        label.Text(winrt::hstring(node.name));
        label.Width((std::max)(80.0, 300.0 - indent_width));
        label.TextTrimming(TextTrimming::CharacterEllipsis);
        label.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(label);

        if (node.is_more_row) {
            Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
                item, winrt::hstring(node.name));
            item.Content(row);
            return item;
        }

        // Size-proportion bar and percentage relative to the parent.
        double percent = 100.0;
        if (node.parent != SIZE_MAX && node.parent < m_tree_nodes.size()) {
            const auto parent_physical = m_tree_nodes[node.parent].physical_bytes;
            percent = parent_physical > 0
                ? (static_cast<double>(node.physical_bytes) / static_cast<double>(parent_physical)) * 100.0
                : 0.0;
        }
        percent = std::clamp(percent, 0.0, 100.0);

        auto track = Border{};
        track.Width(150.0);
        track.Height(8.0);
        track.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
        track.Background(MakeBrush(ActiveShellTheme().progress_track));
        track.VerticalAlignment(VerticalAlignment::Center);

        auto fill = Border{};
        fill.Width(1.5 * percent);
        fill.Height(8.0);
        fill.CornerRadius(CornerRadius{ 4.0, 4.0, 4.0, 4.0 });
        fill.Background(MakeBrush(ActiveShellTheme().progress_fill));
        fill.HorizontalAlignment(HorizontalAlignment::Left);
        track.Child(fill);
        row.Children().Append(track);

        auto append_cell = [&](std::wstring const& text, double min_width, double opacity) {
            auto cell = TextBlock{};
            cell.Text(winrt::hstring(text));
            cell.MinWidth(min_width);
            cell.Opacity(opacity);
            cell.VerticalAlignment(VerticalAlignment::Center);
            row.Children().Append(cell);
        };

        // Rollups don't exist until the scan finishes, so live-tree rows
        // show pending markers instead of misleading zeros.
        const bool totals_pending = m_session_active && node.physical_bytes == 0;

        wchar_t percent_text[16]{};
        swprintf_s(percent_text, L"%.1f%%", percent);
        append_cell(totals_pending ? std::wstring(L"...") : std::wstring(percent_text), 52.0, 0.8);
        append_cell(totals_pending ? std::wstring(L"...") : FormatBytes(node.physical_bytes), 84.0, 0.8);
        append_cell(totals_pending ? std::wstring(L"...") : FormatBytes(node.logical_bytes), 84.0, 0.8);
        append_cell(
            (node.is_directory && !totals_pending) ? std::to_wstring(node.file_count) : std::wstring(L"-"),
            64.0,
            0.72);
        append_cell(
            node.has_modified_utc ? FormatFileTimeUtc(node.modified_utc) : std::wstring(L"-"),
            120.0,
            0.72);

        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            item,
            winrt::hstring(
                node.name + L", " + (node.is_directory ? L"Folder" : L"File") +
                L", " + (totals_pending ? std::wstring(L"scanning") : FormatBytes(node.physical_bytes))));

        item.Content(row);
        return item;
    }

    void MainWindow::OpenExternal(std::wstring const& target)
    {
        ShellExecuteW(nullptr, L"open", target.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
    }

    // Shows exactly the cards belonging to the active sidebar view. The
    // explorer view additionally honors the per-panel toggles.
    void MainWindow::ApplyViewVisibility()
    {
        const bool explorer = m_active_view == AppView::Explorer;
        SetControlVisibility(OverviewCard(), explorer && m_show_current_state);
        SetControlVisibility(TreeCard(), explorer && m_show_folder_tree);
        SetControlVisibility(SearchCard(), explorer && m_show_search);
        SetControlVisibility(DiagnosticsCard(), explorer && m_show_runtime_metrics);
        SetControlVisibility(TreemapCard(), explorer);
        SetControlVisibility(DetailCard(), explorer && m_show_folder_view);
        if (ExtensionCard()) {
            // The extension card sits inside the right panel next to the
            // treemap; hide the whole panel outside the explorer view.
            auto parent = ExtensionCard().Parent().try_as<Microsoft::UI::Xaml::FrameworkElement>();
            if (parent) {
                SetControlVisibility(parent, explorer);
            }
        }
        SetControlVisibility(m_dashboard_card, m_active_view == AppView::Dashboard);
        SetControlVisibility(m_insights_card, m_active_view == AppView::Insights);
        SetControlVisibility(m_cleanup_card, m_active_view == AppView::Cleanup);
        SetControlVisibility(m_settings_card, m_active_view == AppView::Settings);
        SetControlVisibility(m_support_card, m_active_view == AppView::Support);
    }

    void MainWindow::SwitchView(AppView view)
    {
        m_active_view = view;

        // Restyle the sidebar rail: active item gets the red pill.
        auto const& theme = ActiveShellTheme();
        for (auto const& [item_view, item] : m_sidebar_items) {
            if (!item) {
                continue;
            }
            const bool active = item_view == view;
            item.Background(MakeBrush(active
                ? theme.chip_active_background
                : Windows::UI::Colors::Transparent()));
            item.Foreground(MakeBrush(active ? theme.text_on_accent : theme.text_secondary));
            item.FontWeight({ active ? uint16_t{ 700 } : uint16_t{ 400 } });
        }

        switch (view) {
        case AppView::Dashboard:
            PopulateDashboardView();
            UpdateStatus(L"Dashboard view.");
            break;
        case AppView::Insights:
            PopulateInsightsView();
            UpdateStatus(L"Insights view.");
            break;
        case AppView::Cleanup:
            PopulateCleanupView();
            UpdateStatus(L"Cleanup center.");
            break;
        case AppView::Settings:
            PopulateSettingsView();
            UpdateStatus(L"Settings.");
            break;
        case AppView::Support:
            PopulateSupportView();
            UpdateStatus(L"Support.");
            break;
        case AppView::Explorer:
        default:
            UpdateStatus(L"Explorer view.");
            break;
        }
        ApplyViewVisibility();
    }

    namespace
    {
        // Shared helpers for the sidebar views.
        struct DriveSpace
        {
            uint64_t total_bytes{ 0 };
            uint64_t free_bytes{ 0 };
            bool valid{ false };
        };

        DriveSpace QueryDriveSpace(std::wstring const& root_path)
        {
            DriveSpace space;
            std::wstring root = root_path.empty() ? L"C:\\" : root_path;
            if (root.size() >= 2 && root[1] == L':') {
                root = root.substr(0, 2) + L"\\";
            }
            ULARGE_INTEGER free_to_caller{}, total{}, total_free{};
            if (GetDiskFreeSpaceExW(root.c_str(), &free_to_caller, &total, &total_free)) {
                space.total_bytes = total.QuadPart;
                space.free_bytes = total_free.QuadPart;
                space.valid = true;
            }
            return space;
        }
    }

    void MainWindow::PopulateDashboardView()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        if (!m_dashboard_content) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto& content = m_dashboard_content;
        content.Children().Clear();

        content.Children().Append(MakeCardTitle(L"Storage dashboard"));

        const auto space = QueryDriveSpace(m_current_root_path);
        const uint64_t used = space.valid ? space.total_bytes - space.free_bytes : 0;
        const double used_percent = (space.valid && space.total_bytes > 0)
            ? (static_cast<double>(used) / static_cast<double>(space.total_bytes)) * 100.0
            : 0.0;

        // Stat cards row: total / used / free.
        auto stat_row = StackPanel{};
        stat_row.Orientation(Orientation::Horizontal);
        stat_row.Spacing(14.0);
        auto make_stat = [&](std::wstring_view label, std::wstring const& value, std::wstring const& caption) {
            auto card = Border{};
            ApplyCompactCardStyle(card);
            card.MinWidth(220.0);
            auto stack = StackPanel{};
            stack.Spacing(4.0);
            stack.Children().Append(MakeCardTitle(label));
            auto value_text = TextBlock{};
            value_text.Text(winrt::hstring(value));
            value_text.FontSize(26.0);
            value_text.FontWeight({ 700 });
            value_text.Foreground(MakeBrush(theme.text_primary));
            stack.Children().Append(value_text);
            auto caption_text = TextBlock{};
            caption_text.Text(winrt::hstring(caption));
            caption_text.FontSize(12.0);
            caption_text.Foreground(MakeBrush(theme.text_secondary));
            stack.Children().Append(caption_text);
            card.Child(stack);
            stat_row.Children().Append(card);
        };
        wchar_t percent_text[32]{};
        swprintf_s(percent_text, L"%.1f%% of total capacity", used_percent);
        make_stat(L"Total capacity", space.valid ? FormatBytes(space.total_bytes) : L"-",
            m_current_root_path.empty() ? L"C:\\" : m_current_root_path);
        make_stat(L"Used space", space.valid ? FormatBytes(used) : L"-", percent_text);
        make_stat(L"Free space", space.valid ? FormatBytes(space.free_bytes) : L"-",
            L"Available for new data");
        content.Children().Append(stat_row);

        // Disk usage meter.
        auto meter_card = Border{};
        ApplyCompactCardStyle(meter_card);
        auto meter_stack = StackPanel{};
        meter_stack.Spacing(8.0);
        meter_stack.Children().Append(MakeCardTitle(L"Disk usage"));
        auto meter_value = TextBlock{};
        wchar_t busy_text[32]{};
        swprintf_s(busy_text, L"%.0f%%", used_percent);
        meter_value.Text(winrt::hstring(busy_text));
        meter_value.FontSize(44.0);
        meter_value.FontWeight({ 800 });
        meter_value.Foreground(MakeBrush(theme.chip_active_background));
        meter_stack.Children().Append(meter_value);
        auto meter_track = Border{};
        meter_track.Height(10.0);
        meter_track.CornerRadius(UniformRadius(theme.progress_radius));
        meter_track.Background(MakeBrush(theme.progress_track));
        meter_track.HorizontalAlignment(HorizontalAlignment::Stretch);
        auto meter_fill = Border{};
        meter_fill.Height(10.0);
        meter_fill.Width(6.0 * used_percent);
        meter_fill.CornerRadius(UniformRadius(theme.progress_radius));
        meter_fill.Background(MakeBrush(theme.progress_fill));
        meter_fill.HorizontalAlignment(HorizontalAlignment::Left);
        meter_track.Child(meter_fill);
        meter_stack.Children().Append(meter_track);
        meter_card.Child(meter_stack);
        content.Children().Append(meter_card);

        // Content distribution from extension stats.
        auto distribution_card = Border{};
        ApplyCompactCardStyle(distribution_card);
        auto distribution_stack = StackPanel{};
        distribution_stack.Spacing(8.0);
        distribution_stack.Children().Append(MakeCardTitle(L"Content distribution"));
        if (m_extension_stats.empty()) {
            auto empty_text = TextBlock{};
            empty_text.Text(L"Run a scan to populate the by-extension breakdown.");
            empty_text.Foreground(MakeBrush(theme.text_secondary));
            distribution_stack.Children().Append(empty_text);
        } else {
            uint64_t total_bytes = 0;
            for (auto const& stat : m_extension_stats) {
                total_bytes += stat.bytes;
            }
            auto bar = StackPanel{};
            bar.Orientation(Orientation::Horizontal);
            bar.Height(14.0);
            const size_t segments = (std::min)(static_cast<size_t>(6), m_extension_stats.size());
            for (size_t index = 0; index < segments && total_bytes > 0; ++index) {
                auto const& stat = m_extension_stats[index];
                auto segment = Border{};
                segment.Height(14.0);
                segment.Width((std::max)(4.0,
                    720.0 * static_cast<double>(stat.bytes) / static_cast<double>(total_bytes)));
                segment.Background(MakeBrush(ExtensionSwatchColor(stat.extension)));
                bar.Children().Append(segment);
            }
            distribution_stack.Children().Append(bar);
            for (size_t index = 0; index < segments; ++index) {
                auto const& stat = m_extension_stats[index];
                auto legend_row = StackPanel{};
                legend_row.Orientation(Orientation::Horizontal);
                legend_row.Spacing(8.0);
                auto swatch = Border{};
                swatch.Width(10.0);
                swatch.Height(10.0);
                swatch.CornerRadius(CornerRadius{ 2.0, 2.0, 2.0, 2.0 });
                swatch.Background(MakeBrush(ExtensionSwatchColor(stat.extension)));
                swatch.VerticalAlignment(VerticalAlignment::Center);
                legend_row.Children().Append(swatch);
                auto legend_text = TextBlock{};
                const std::wstring extension_label =
                    stat.extension.empty() ? L"(no extension)" : L"." + stat.extension;
                legend_text.Text(winrt::hstring(
                    extension_label + L"  " + FormatBytes(stat.bytes) + L"  (" +
                    std::to_wstring(stat.files) + L" files)"));
                legend_text.Foreground(MakeBrush(theme.text_primary));
                legend_row.Children().Append(legend_text);
                distribution_stack.Children().Append(legend_row);
            }
        }
        distribution_card.Child(distribution_stack);
        content.Children().Append(distribution_card);

        // Recent activity.
        auto activity_card = Border{};
        ApplyCompactCardStyle(activity_card);
        auto activity_stack = StackPanel{};
        activity_stack.Spacing(6.0);
        activity_stack.Children().Append(MakeCardTitle(L"Recent activity"));
        auto add_activity_line = [&](std::wstring const& line) {
            if (line.empty()) {
                return;
            }
            auto activity_text = TextBlock{};
            activity_text.Text(winrt::hstring(line));
            activity_text.Foreground(MakeBrush(theme.text_secondary));
            activity_text.TextWrapping(TextWrapping::WrapWholeWords);
            activity_stack.Children().Append(activity_text);
        };
        add_activity_line(m_last_scan_duration_text);
        add_activity_line(m_last_cache_load_text);
        if (TreeArenaActive() && !m_tree_nodes.empty()) {
            add_activity_line(
                L"Indexed tree: " + FormatBytes(m_tree_nodes[0].physical_bytes) +
                L" across " + std::to_wstring(m_tree_nodes[0].item_count) + L" items.");
        }
        activity_card.Child(activity_stack);
        content.Children().Append(activity_card);
    }

    void MainWindow::PopulateInsightsView()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        if (!m_insights_content) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto& content = m_insights_content;
        content.Children().Clear();

        content.Children().Append(MakeCardTitle(L"Storage insights"));

        // Top directories from the loaded tree root.
        auto top_card = Border{};
        ApplyCompactCardStyle(top_card);
        auto top_stack = StackPanel{};
        top_stack.Spacing(6.0);
        top_stack.Children().Append(MakeCardTitle(L"Top directories"));
        if (!TreeArenaActive() || m_tree_nodes.empty() || m_tree_nodes[0].children.empty()) {
            auto empty_text = TextBlock{};
            empty_text.Text(L"Run a scan to analyze the largest directories.");
            empty_text.Foreground(MakeBrush(theme.text_secondary));
            top_stack.Children().Append(empty_text);
        } else {
            const uint64_t root_physical =
                (std::max)(static_cast<uint64_t>(1), m_tree_nodes[0].physical_bytes);
            size_t shown = 0;
            for (size_t child_index : m_tree_nodes[0].children) {
                if (shown >= 12 || child_index >= m_tree_nodes.size()) {
                    break;
                }
                auto const& node = m_tree_nodes[child_index];
                if (node.is_more_row) {
                    continue;
                }
                const double percent =
                    100.0 * static_cast<double>(node.physical_bytes) / static_cast<double>(root_physical);
                auto row = StackPanel{};
                row.Orientation(Orientation::Horizontal);
                row.Spacing(12.0);
                auto name_text = TextBlock{};
                name_text.Text(winrt::hstring(node.name));
                name_text.Width(240.0);
                name_text.TextTrimming(TextTrimming::CharacterEllipsis);
                name_text.Foreground(MakeBrush(theme.text_primary));
                row.Children().Append(name_text);
                auto track = Border{};
                track.Width(180.0);
                track.Height(8.0);
                track.VerticalAlignment(VerticalAlignment::Center);
                track.CornerRadius(UniformRadius(theme.progress_radius));
                track.Background(MakeBrush(theme.progress_track));
                auto fill = Border{};
                fill.Height(8.0);
                fill.Width(1.8 * percent);
                fill.HorizontalAlignment(HorizontalAlignment::Left);
                fill.CornerRadius(UniformRadius(theme.progress_radius));
                fill.Background(MakeBrush(theme.progress_fill));
                track.Child(fill);
                row.Children().Append(track);
                wchar_t percent_text[16]{};
                swprintf_s(percent_text, L"%.1f%%", percent);
                auto percent_block = TextBlock{};
                percent_block.Text(winrt::hstring(percent_text));
                percent_block.MinWidth(56.0);
                percent_block.Foreground(MakeBrush(theme.text_secondary));
                row.Children().Append(percent_block);
                auto size_block = TextBlock{};
                size_block.Text(winrt::hstring(FormatBytes(node.physical_bytes)));
                size_block.MinWidth(90.0);
                size_block.Foreground(MakeBrush(theme.text_primary));
                row.Children().Append(size_block);
                auto items_block = TextBlock{};
                items_block.Text(winrt::hstring(std::to_wstring(node.item_count) + L" items"));
                items_block.Foreground(MakeBrush(theme.text_secondary));
                row.Children().Append(items_block);
                top_stack.Children().Append(row);
                ++shown;
            }
        }
        top_card.Child(top_stack);
        content.Children().Append(top_card);

        // Extension breakdown table.
        auto extension_card = Border{};
        ApplyCompactCardStyle(extension_card);
        auto extension_stack = StackPanel{};
        extension_stack.Spacing(6.0);
        extension_stack.Children().Append(MakeCardTitle(L"Extension breakdown"));
        if (m_extension_stats.empty()) {
            auto empty_text = TextBlock{};
            empty_text.Text(L"Run a scan to populate extension statistics.");
            empty_text.Foreground(MakeBrush(theme.text_secondary));
            extension_stack.Children().Append(empty_text);
        } else {
            uint64_t total_bytes = 0;
            for (auto const& stat : m_extension_stats) {
                total_bytes += stat.bytes;
            }
            const size_t rows = (std::min)(static_cast<size_t>(15), m_extension_stats.size());
            for (size_t index = 0; index < rows; ++index) {
                auto const& stat = m_extension_stats[index];
                auto row = StackPanel{};
                row.Orientation(Orientation::Horizontal);
                row.Spacing(10.0);
                auto swatch = Border{};
                swatch.Width(10.0);
                swatch.Height(10.0);
                swatch.CornerRadius(CornerRadius{ 2.0, 2.0, 2.0, 2.0 });
                swatch.Background(MakeBrush(ExtensionSwatchColor(stat.extension)));
                swatch.VerticalAlignment(VerticalAlignment::Center);
                row.Children().Append(swatch);
                auto extension_text = TextBlock{};
                extension_text.Text(winrt::hstring(
                    stat.extension.empty() ? L"(none)" : L"." + stat.extension));
                extension_text.Width(84.0);
                extension_text.Foreground(MakeBrush(theme.text_primary));
                row.Children().Append(extension_text);
                auto size_text = TextBlock{};
                size_text.Text(winrt::hstring(FormatBytes(stat.bytes)));
                size_text.Width(96.0);
                size_text.Foreground(MakeBrush(theme.text_primary));
                row.Children().Append(size_text);
                auto ratio_text = TextBlock{};
                const double ratio = total_bytes > 0
                    ? 100.0 * static_cast<double>(stat.bytes) / static_cast<double>(total_bytes)
                    : 0.0;
                wchar_t ratio_buffer[16]{};
                swprintf_s(ratio_buffer, L"%.2f%%", ratio);
                ratio_text.Text(winrt::hstring(ratio_buffer));
                ratio_text.Width(72.0);
                ratio_text.Foreground(MakeBrush(theme.folder_accent));
                row.Children().Append(ratio_text);
                auto files_text = TextBlock{};
                files_text.Text(winrt::hstring(std::to_wstring(stat.files) + L" files"));
                files_text.Foreground(MakeBrush(theme.text_secondary));
                row.Children().Append(files_text);
                extension_stack.Children().Append(row);
            }
        }
        extension_card.Child(extension_stack);
        content.Children().Append(extension_card);
    }

    void MainWindow::PopulateCleanupView()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        if (!m_cleanup_content) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto& content = m_cleanup_content;
        content.Children().Clear();

        content.Children().Append(MakeCardTitle(L"Cleanup center"));

        auto intro = TextBlock{};
        intro.Text(L"Reclaim space by reviewing the biggest opportunities below. WinBlaze never deletes anything; use Open to inspect items in File Explorer.");
        intro.Foreground(MakeBrush(theme.text_secondary));
        intro.TextWrapping(TextWrapping::WrapWholeWords);
        content.Children().Append(intro);

        // Temp/log potential from extension stats.
        auto temp_card = Border{};
        ApplyCompactCardStyle(temp_card);
        auto temp_stack = StackPanel{};
        temp_stack.Spacing(6.0);
        temp_stack.Children().Append(MakeCardTitle(L"Temporary and log files"));
        uint64_t temp_bytes = 0;
        uint64_t temp_files = 0;
        for (auto const& stat : m_extension_stats) {
            if (stat.extension == L"tmp" || stat.extension == L"log" || stat.extension == L"bak" ||
                stat.extension == L"old" || stat.extension == L"dmp" || stat.extension == L"cache") {
                temp_bytes += stat.bytes;
                temp_files += stat.files;
            }
        }
        auto temp_value = TextBlock{};
        temp_value.Text(winrt::hstring(
            L"Potential gain: " + FormatBytes(temp_bytes) + L" across " +
            std::to_wstring(temp_files) + L" .tmp/.log/.bak/.old/.dmp/.cache files"));
        temp_value.Foreground(MakeBrush(theme.folder_accent));
        temp_value.FontWeight({ 600 });
        temp_stack.Children().Append(temp_value);
        temp_card.Child(temp_stack);
        content.Children().Append(temp_card);

        // Largest files with Open actions.
        auto large_card = Border{};
        ApplyCompactCardStyle(large_card);
        auto large_stack = StackPanel{};
        large_stack.Spacing(6.0);
        large_stack.Children().Append(MakeCardTitle(L"Largest files"));
        std::vector<std::pair<std::wstring, uint64_t>> largest;
        try {
            ::WinBlaze::UI::NativeBridge::TreeLargestFiles(50, [&largest](WbTreeNode const& node) {
                const std::wstring path = node.name.ptr == nullptr
                    ? std::wstring{}
                    : Utf8ToWide(std::string_view{ node.name.ptr, node.name.len });
                largest.emplace_back(path, node.physical_bytes);
            });
        } catch (...) {
        }
        if (largest.empty()) {
            auto empty_text = TextBlock{};
            empty_text.Text(L"Run a scan to identify the largest files.");
            empty_text.Foreground(MakeBrush(theme.text_secondary));
            large_stack.Children().Append(empty_text);
        } else {
            for (auto const& [file_path, bytes] : largest) {
                auto row = StackPanel{};
                row.Orientation(Orientation::Horizontal);
                row.Spacing(10.0);
                auto size_text = TextBlock{};
                size_text.Text(winrt::hstring(FormatBytes(bytes)));
                size_text.MinWidth(88.0);
                size_text.FontWeight({ 600 });
                size_text.Foreground(MakeBrush(theme.text_primary));
                size_text.VerticalAlignment(VerticalAlignment::Center);
                row.Children().Append(size_text);
                auto path_text = TextBlock{};
                path_text.Text(winrt::hstring(file_path));
                path_text.Width(620.0);
                path_text.TextTrimming(TextTrimming::CharacterEllipsis);
                path_text.Foreground(MakeBrush(theme.text_secondary));
                path_text.VerticalAlignment(VerticalAlignment::Center);
                row.Children().Append(path_text);
                auto open_button = Button{};
                open_button.Content(box_value(L"Open"));
                open_button.FontSize(12.0);
                const std::wstring captured_path = file_path;
                open_button.Click([captured_path](auto const&, auto const&) {
                    ShellExecuteW(nullptr, L"open", L"explorer.exe",
                        (L"/select,\"" + captured_path + L"\"").c_str(), nullptr, SW_SHOWNORMAL);
                });
                row.Children().Append(open_button);
                large_stack.Children().Append(row);
            }
        }
        large_card.Child(large_stack);
        content.Children().Append(large_card);
    }

    void MainWindow::PopulateSettingsView()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        if (!m_settings_content) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto& content = m_settings_content;
        content.Children().Clear();

        content.Children().Append(MakeCardTitle(L"Settings"));

        auto add_setting = [&](std::wstring_view label, std::wstring const& value) {
            auto card = Border{};
            ApplyCompactCardStyle(card);
            auto stack = StackPanel{};
            stack.Spacing(4.0);
            stack.Children().Append(MakeCardTitle(label));
            auto value_text = TextBlock{};
            value_text.Text(winrt::hstring(value));
            value_text.Foreground(MakeBrush(theme.text_primary));
            value_text.TextWrapping(TextWrapping::WrapWholeWords);
            stack.Children().Append(value_text);
            card.Child(stack);
            content.Children().Append(card);
            return stack;
        };

        if (IsProcessElevated()) {
            add_setting(L"Process elevation",
                L"Running as administrator: the raw NTFS MFT fast path is available on every local volume.");
        } else {
            auto elevation_stack = add_setting(L"Process elevation",
                L"Running without administrator rights. Scans still work: WinBlaze reads the MFT when the "
                L"volume allows it and otherwise falls back to a directory walk. Restart elevated to "
                L"guarantee the MFT fast path.");
            auto elevate_button = Button{};
            elevate_button.Content(box_value(L"Restart as administrator"));
            elevate_button.Click([](auto const&, auto const&) {
                if (RelaunchElevated()) {
                    Application::Current().Exit();
                }
            });
            elevation_stack.Children().Append(elevate_button);
        }

        {
            auto updates_stack = add_setting(L"Updates",
                L"Check GitHub for a newer WinBlaze release. Downloads open in your browser.");
            auto update_status = TextBlock{};
            update_status.Text(winrt::hstring(
                L"Current version " + std::wstring(winrt::to_hstring(kCurrentVersion).c_str()) + L"."));
            update_status.Foreground(MakeBrush(theme.text_primary));
            update_status.TextWrapping(TextWrapping::WrapWholeWords);
            updates_stack.Children().Append(update_status);

            auto button_row = StackPanel{};
            button_row.Orientation(Orientation::Horizontal);
            button_row.Spacing(10.0);
            auto check_button = Button{};
            check_button.Content(box_value(L"Check for updates"));
            auto download_button = Button{};
            download_button.Content(box_value(L"Download update"));
            download_button.Visibility(Visibility::Collapsed);
            download_button.Click([](auto const&, auto const&) {
                ShellExecuteW(nullptr, L"open", kReleasesLatestUrl, nullptr, nullptr, SW_SHOWNORMAL);
            });
            check_button.Click([update_status, download_button](auto const&, auto const&) {
                CheckForUpdatesAsync(update_status, download_button);
            });
            button_row.Children().Append(check_button);
            button_row.Children().Append(download_button);
            updates_stack.Children().Append(button_row);
        }

        add_setting(L"Theme", L"High Velocity (red on black, from the Stitch design system)");
        add_setting(L"Reparse points",
            L"Skipped by default: junction and symlink targets are counted at their real location, which prevents double counting.");
        add_setting(L"Scanning",
            L"Raw NTFS MFT fast path when running as Administrator; parallel directory walk otherwise.");

        auto data_stack = add_setting(L"Data locations",
            L"Logs and the scan index are stored locally; WinBlaze has no telemetry.");
        auto open_row = StackPanel{};
        open_row.Orientation(Orientation::Horizontal);
        open_row.Spacing(10.0);
        auto make_open_button = [&](std::wstring_view label, std::wstring const& path) {
            auto button = Button{};
            button.Content(box_value(winrt::hstring(label)));
            const std::wstring captured = path;
            button.Click([captured](auto const&, auto const&) {
                ShellExecuteW(nullptr, L"open", captured.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
            });
            open_row.Children().Append(button);
        };
        wchar_t local_app_data[MAX_PATH]{};
        if (GetEnvironmentVariableW(L"LOCALAPPDATA", local_app_data, MAX_PATH) > 0) {
            const std::wstring base = std::wstring(local_app_data) + L"\\WinBlaze";
            make_open_button(L"Open logs folder", base + L"\\logs");
            make_open_button(L"Open index folder", base + L"\\index");
        }
        data_stack.Children().Append(open_row);
    }

    void MainWindow::PopulateSupportView()
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;
        if (!m_support_content) {
            return;
        }
        auto const& theme = ActiveShellTheme();
        auto& content = m_support_content;
        content.Children().Clear();

        content.Children().Append(MakeCardTitle(L"Support"));

        auto blurb = TextBlock{};
        blurb.Text(L"WinBlaze is open source. Bug reports and feature requests are welcome on GitHub; logs live under %LOCALAPPDATA%\\WinBlaze\\logs if you need to attach evidence.");
        blurb.Foreground(MakeBrush(theme.text_secondary));
        blurb.TextWrapping(TextWrapping::WrapWholeWords);
        content.Children().Append(blurb);

        auto link_row = StackPanel{};
        link_row.Orientation(Orientation::Horizontal);
        link_row.Spacing(10.0);
        auto make_link_button = [&](std::wstring_view label, std::wstring const& url, bool primary) {
            auto button = Button{};
            button.Content(box_value(winrt::hstring(label)));
            if (primary) {
                button.Background(MakeBrush(theme.chip_active_background));
                button.Foreground(MakeBrush(theme.text_on_accent));
                button.FontWeight({ 700 });
            }
            const std::wstring captured = url;
            button.Click([captured](auto const&, auto const&) {
                OpenExternal(captured);
            });
            link_row.Children().Append(button);
        };
        make_link_button(L"GitHub repository", L"https://github.com/marksmayo/WinBlaze", true);
        make_link_button(L"Report an issue", L"https://github.com/marksmayo/WinBlaze/issues", false);
        make_link_button(L"Discussions", L"https://github.com/marksmayo/WinBlaze/discussions", false);
        content.Children().Append(link_row);
    }

    MainWindow::ExtensionStatEntry MainWindow::ExtensionStatFromNative(WbExtensionStat const& entry)
    {
        auto to_wide = [](WbCStringView view) {
            if (view.ptr == nullptr || view.len == 0) {
                return std::wstring{};
            }
            return Utf8ToWide(std::string_view{ view.ptr, view.len });
        };

        ExtensionStatEntry stat;
        stat.extension = to_wide(entry.extension);
        stat.description = to_wide(entry.description);
        stat.bytes = entry.bytes;
        stat.files = entry.files;
        return stat;
    }

    Microsoft::UI::Xaml::Controls::ListViewItem MainWindow::CreateExtensionListItem(
        ExtensionStatEntry const& entry,
        uint64_t total_bytes) const
    {
        using namespace Microsoft::UI::Xaml;
        using namespace Microsoft::UI::Xaml::Controls;

        const std::wstring display_extension = entry.extension.empty() ? L"(none)" : entry.extension;
        const double percentage = total_bytes > 0
            ? (static_cast<double>(entry.bytes) / static_cast<double>(total_bytes)) * 100.0
            : 0.0;

        auto item = ListViewItem{};
        Microsoft::UI::Xaml::Automation::AutomationProperties::SetName(
            item,
            winrt::hstring(display_extension + L", " + entry.description + L", " + FormatBytes(entry.bytes)));

        auto row = StackPanel{};
        row.Orientation(Orientation::Horizontal);
        row.Spacing(10);

        auto swatch = Border{};
        swatch.Width(12.0);
        swatch.Height(12.0);
        swatch.CornerRadius(CornerRadius{ 3.0, 3.0, 3.0, 3.0 });
        swatch.Background(MakeBrush(ExtensionSwatchColor(entry.extension)));
        swatch.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(swatch);

        auto extension_text = TextBlock{};
        extension_text.Text(winrt::hstring(display_extension));
        extension_text.Width(56.0);
        extension_text.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(extension_text);

        auto description_text = TextBlock{};
        description_text.Text(winrt::hstring(entry.description));
        description_text.Width(168.0);
        description_text.Opacity(0.8);
        description_text.TextTrimming(TextTrimming::CharacterEllipsis);
        description_text.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(description_text);

        auto bytes_text = TextBlock{};
        bytes_text.Text(winrt::hstring(FormatBytes(entry.bytes)));
        bytes_text.Width(72.0);
        bytes_text.Opacity(0.8);
        bytes_text.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(bytes_text);

        auto percentage_text = TextBlock{};
        std::wostringstream percentage_stream;
        percentage_stream.setf(std::ios::fixed);
        percentage_stream.precision(1);
        percentage_stream << percentage;
        percentage_text.Text(winrt::hstring(percentage_stream.str() + L"%"));
        percentage_text.Width(44.0);
        percentage_text.Opacity(0.8);
        percentage_text.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(percentage_text);

        auto files_text = TextBlock{};
        files_text.Text(winrt::hstring(std::to_wstring(entry.files)));
        files_text.Width(56.0);
        files_text.Opacity(0.72);
        files_text.VerticalAlignment(VerticalAlignment::Center);
        row.Children().Append(files_text);

        item.Content(row);
        return item;
    }

    void MainWindow::PopulateExtensionList(std::vector<ExtensionStatEntry> const& entries)
    {
        if (!ExtensionListView()) {
            return;
        }

        uint64_t total_bytes = 0;
        for (auto const& entry : entries) {
            total_bytes += entry.bytes;
        }

        ExtensionListView().Items().Clear();
        for (auto const& entry : entries) {
            ExtensionListView().Items().Append(CreateExtensionListItem(entry, total_bytes));
        }

        if (ExtensionListStatusText()) {
            ExtensionListStatusText().Text(winrt::hstring(
                entries.empty()
                    ? L"Waiting for scan data."
                    : L"Showing " + std::to_wstring(entries.size()) +
                        L" extensions, sorted by bytes descending."));
        }
    }

    void MainWindow::LoadExtensionStatsSnapshot()
    {
        std::vector<ExtensionStatEntry> entries;
        ::WinBlaze::UI::NativeBridge::LoadExtensionStatsSnapshot(
            [&entries](WbExtensionStat const& entry) {
                entries.push_back(ExtensionStatFromNative(entry));
            });
        m_extension_stats = std::move(entries);
        PopulateExtensionList(m_extension_stats);
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
