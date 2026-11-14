#include "pch.h"
#include "NativeBridge.h"
#include "StartupTrace.h"

#include <atomic>
#include <condition_variable>
#include <stdexcept>
#include <filesystem>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <shellapi.h>

namespace
{
    using StartScanFn = WbScanSessionHandle(__stdcall *)(WbCStringView, WbEventCallback, void*);
    using CancelScanFn = void(__stdcall *)(WbScanSessionHandle);
    using DestroyScanFn = void(__stdcall *)(WbScanSessionHandle);
    using LoadCatalogSnapshotWithStatsFn = WbIndexSnapshotStats(__stdcall *)(WbCatalogCallback, void*);
    using LoadExtensionStatsFn = void(__stdcall *)(WbExtensionStatCallback, void*);
    using TreeRootFn = uint8_t(__stdcall *)(WbTreeNodeCallback, void*);
    using ExportCsvFilteredFn = uint32_t(__stdcall *)(WbCStringView, WbCsvFilter);
    using ExportCsvExtensionFn = uint32_t(__stdcall *)(WbCStringView, WbCStringView);
    using ImportCsvOptionsFn = uint32_t(__stdcall *)(WbCStringView, uint32_t, uint32_t);
    using ExportCsvFn = uint32_t(__stdcall *)(WbCStringView);
    using TreeChildrenFn = WbTreeChildrenResult(__stdcall *)(uint64_t, uint64_t, WbTreeNodeCallback, void*);
    using TreeLargestFilesPageFn = WbTreeChildrenResult(__stdcall *)(uint64_t, uint64_t, WbTreeNodeCallback, void*);
    using TreeLargestFilesFn = void(__stdcall *)(uint64_t, WbTreeNodeCallback, void*);
    using UpdateCheckFn = WbUpdateCheck(__stdcall *)(WbCStringView, WbCStringView);
    using VerifyDownloadFn = uint8_t(__stdcall *)(WbCStringView, WbCStringView, WbCStringView);

    struct DllApi
    {
        HMODULE module{ nullptr };
        StartScanFn start_scan{ nullptr };
        StartScanFn start_incremental_scan{ nullptr };
        CancelScanFn cancel_scan{ nullptr };
        DestroyScanFn destroy_scan{ nullptr };
        LoadCatalogSnapshotWithStatsFn load_catalog_snapshot_with_stats{ nullptr };
        LoadExtensionStatsFn load_extension_stats{ nullptr };
        TreeRootFn tree_root{ nullptr };
        ImportCsvOptionsFn import_csv{ nullptr };
        ExportCsvFn export_csv{ nullptr };
        ExportCsvFn export_json{ nullptr };
        ExportCsvFilteredFn export_csv_filtered{ nullptr };
        ExportCsvExtensionFn export_csv_extension{ nullptr };
        TreeChildrenFn tree_children{ nullptr };
        TreeLargestFilesPageFn tree_largest_files_page{ nullptr };
        TreeLargestFilesFn tree_largest_files{ nullptr };
        UpdateCheckFn update_check{ nullptr };
        VerifyDownloadFn verify_download{ nullptr };
    };

    std::atomic<uint32_t> g_import_runs{ 0 };
    std::atomic<uint32_t> g_import_prestarts{ 0 };

    DllApi& Api()
    {
        static DllApi api;
        return api;
    }

    std::filesystem::path ExeDirectory()
    {
        std::wstring buffer(MAX_PATH, L'\0');
        const auto length = GetModuleFileNameW(nullptr, buffer.data(), static_cast<DWORD>(buffer.size()));
        if (length == 0) {
            return {};
        }

        buffer.resize(length);
        return std::filesystem::path(buffer).parent_path();
    }

    struct CallbackContext
    {
        WinBlaze::UI::NativeBridge::EventHandler handler;
    };

    struct CatalogCallbackContext
    {
        WinBlaze::UI::NativeBridge::CatalogHandler handler;
    };

    struct ExtensionStatCallbackContext
    {
        WinBlaze::UI::NativeBridge::ExtensionStatHandler handler;
    };

    struct TreeNodeCallbackContext
    {
        WinBlaze::UI::NativeBridge::TreeNodeHandler handler;
    };

    void EnsureLoaded()
    {
        // A launch-time import may load the engine from a worker thread.
        static std::mutex load_mutex;
        std::lock_guard guard(load_mutex);
        auto& api = Api();
        if (api.module != nullptr) {
            return;
        }

        const auto dll_path = ExeDirectory() / L"winblaze_native.dll";
        api.module = LoadLibraryW(dll_path.c_str());
        if (api.module == nullptr) {
            throw std::runtime_error("Failed to load winblaze_native.dll");
        }

        api.start_scan = reinterpret_cast<StartScanFn>(GetProcAddress(api.module, "wb_scan_session_start"));
        api.start_incremental_scan = reinterpret_cast<StartScanFn>(GetProcAddress(api.module, "wb_scan_session_start_incremental"));
        api.cancel_scan = reinterpret_cast<CancelScanFn>(GetProcAddress(api.module, "wb_scan_session_cancel"));
        api.destroy_scan = reinterpret_cast<DestroyScanFn>(GetProcAddress(api.module, "wb_scan_session_destroy"));
        api.load_catalog_snapshot_with_stats = reinterpret_cast<LoadCatalogSnapshotWithStatsFn>(GetProcAddress(api.module, "wb_index_snapshot_load_with_stats"));
        api.load_extension_stats = reinterpret_cast<LoadExtensionStatsFn>(GetProcAddress(api.module, "wb_index_snapshot_extension_stats"));
        api.tree_root = reinterpret_cast<TreeRootFn>(GetProcAddress(api.module, "wb_tree_root"));
        api.import_csv = reinterpret_cast<ImportCsvOptionsFn>(GetProcAddress(api.module, "wb_import_scan"));
        api.export_csv = reinterpret_cast<ExportCsvFn>(GetProcAddress(api.module, "wb_export_csv"));
        api.export_json = reinterpret_cast<ExportCsvFn>(GetProcAddress(api.module, "wb_export_json"));
        api.export_csv_filtered = reinterpret_cast<ExportCsvFilteredFn>(GetProcAddress(api.module, "wb_export_csv_filtered"));
        api.export_csv_extension = reinterpret_cast<ExportCsvExtensionFn>(GetProcAddress(api.module, "wb_export_csv_extension"));
        api.tree_children = reinterpret_cast<TreeChildrenFn>(GetProcAddress(api.module, "wb_tree_children"));
        api.tree_largest_files_page = reinterpret_cast<TreeLargestFilesPageFn>(GetProcAddress(api.module, "wb_tree_largest_files_page"));
        api.tree_largest_files = reinterpret_cast<TreeLargestFilesFn>(GetProcAddress(api.module, "wb_tree_largest_files"));
        // Optional: absence (older DLL) degrades the update check gracefully
        // rather than failing app startup, so it is not in the required set.
        api.update_check = reinterpret_cast<UpdateCheckFn>(GetProcAddress(api.module, "wb_update_check"));
        api.verify_download = reinterpret_cast<VerifyDownloadFn>(GetProcAddress(api.module, "wb_verify_download"));

        if (api.start_scan == nullptr || api.start_incremental_scan == nullptr || api.cancel_scan == nullptr || api.destroy_scan == nullptr || api.load_catalog_snapshot_with_stats == nullptr || api.load_extension_stats == nullptr || api.tree_root == nullptr || api.tree_children == nullptr || api.tree_largest_files == nullptr) {
            throw std::runtime_error("Failed to resolve winblaze_native exports");
        }
    }

    extern "C" void __stdcall OnEventWithContext(const WbEvent* event, void* user_data)
    {
        if (event == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<CallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*event);
        }
    }

    extern "C" void __stdcall OnCatalogWithContext(const WbCatalogEntry* entry, void* user_data)
    {
        if (entry == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<CatalogCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*entry);
        }
    }

    extern "C" void __stdcall OnExtensionStatWithContext(const WbExtensionStat* entry, void* user_data)
    {
        if (entry == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<ExtensionStatCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*entry);
        }
    }

    extern "C" void __stdcall OnTreeNodeWithContext(const WbTreeNode* node, void* user_data)
    {
        if (node == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<TreeNodeCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*node);
        }
    }

    std::string ToUtf8(const wchar_t* input)
    {
        if (input == nullptr) {
            return {};
        }

        const int required = WideCharToMultiByte(CP_UTF8, 0, input, -1, nullptr, 0, nullptr, nullptr);
        if (required <= 0) {
            return {};
        }

        std::string output(static_cast<size_t>(required), '\0');
        WideCharToMultiByte(CP_UTF8, 0, input, -1, output.data(), required, nullptr, nullptr);
        output.resize(static_cast<size_t>(required - 1));
        return output;
    }
}

namespace WinBlaze::UI::NativeBridge
{
    void Initialize()
    {
        EnsureLoaded();
    }

    namespace
    {
        struct PrestartState
        {
            std::wstring path;
            bool wiztree_utc{};
            uint32_t result{ 4 };
        };
        struct PrestartedImport
        {
            // Shared with the worker, so the worker never touches freed state.
            std::shared_ptr<PrestartState> state;
            std::thread worker;
            PrestartedImport() = default;
            PrestartedImport(PrestartedImport const&) = delete;
            PrestartedImport& operator=(PrestartedImport const&) = delete;
            // Reached at process exit when no ImportCsv collected the job (the
            // window closed first). A joinable std::thread would terminate the
            // process; the import is abandoned instead of awaited.
            ~PrestartedImport()
            {
                if (worker.joinable()) { worker.detach(); }
            }
        };
        // Outlives the job, which ImportCsv collects and destroys.
        struct PrestartCompletion
        {
            std::mutex mutex;
            std::condition_variable finished;
            bool done{ false };
        };
        std::mutex g_prestart_mutex;
        std::unique_ptr<PrestartedImport> g_prestart;
        std::shared_ptr<PrestartCompletion> g_prestart_completion;
        bool g_prestart_used{ false };

        uint32_t RunImport(const wchar_t* inputPath, bool wiztreeUtc)
        {
            EnsureLoaded();
            if (!Api().import_csv) { return 4; }
            const auto utf8 = ToUtf8(inputPath);
            ++g_import_runs;
            return Api().import_csv(WbCStringView{ utf8.data(), utf8.size() }, 0U, wiztreeUtc ? 1U : 0U);
        }
    }

    void PrestartImportCsv(const wchar_t* inputPath, bool wiztreeUtc)
    {
        std::lock_guard guard(g_prestart_mutex);
        if (g_prestart_used) { return; }
        g_prestart_used = true;
        auto job = std::make_unique<PrestartedImport>();
        job->state = std::make_shared<PrestartState>();
        job->state->path = inputPath;
        job->state->wiztree_utc = wiztreeUtc;
        auto completion = std::make_shared<PrestartCompletion>();
        g_prestart_completion = completion;
        job->worker = std::thread([state = job->state, completion] {
            TraceStartup(L"Prestarted import begin");
            try { state->result = RunImport(state->path.c_str(), state->wiztree_utc); } catch (...) {}
            TraceStartup(L"Prestarted import end");
            {
                std::lock_guard done_guard(completion->mutex);
                completion->done = true;
            }
            completion->finished.notify_all();
        });
        ++g_import_prestarts;
        g_prestart = std::move(job);
    }

    void PrestartCommandLineImport()
    {
        // Same rule as MainWindow: a single argument that is not an option
        // and names an existing file is a saved scan to open.
        int count = 0;
        auto arguments = CommandLineToArgvW(GetCommandLineW(), &count);
        if (!arguments) { return; }
        if (count == 2 && std::wstring_view(arguments[1]).rfind(L"--", 0) != 0) {
            const DWORD attributes = GetFileAttributesW(arguments[1]);
            if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) {
                PrestartImportCsv(arguments[1], false);
            }
        }
        LocalFree(arguments);
    }

    bool WaitForPrestartedImport(std::chrono::milliseconds limit)
    {
        std::shared_ptr<PrestartCompletion> completion;
        {
            std::lock_guard guard(g_prestart_mutex);
            completion = g_prestart_completion;
        }
        if (!completion) { return false; }
        std::unique_lock lock(completion->mutex);
        return completion->finished.wait_for(lock, limit, [&] { return completion->done; });
    }

    uint32_t ImportCsv(const wchar_t* inputPath, bool wiztreeUtc)
    {
        std::unique_ptr<PrestartedImport> job;
        {
            std::lock_guard guard(g_prestart_mutex);
            job = std::move(g_prestart);
        }
        if (job) {
            // Never leave an earlier import running behind a newer one.
            job->worker.join();
            if (job->state->path == inputPath && job->state->wiztree_utc == wiztreeUtc) { return job->state->result; }
        }
        return RunImport(inputPath, wiztreeUtc);
    }

    uint32_t ImportRunCount() { return g_import_runs.load(); }
    uint32_t ImportPrestartCount() { return g_import_prestarts.load(); }

    uint32_t ExportCsv(const wchar_t* outputPath)
    {
        EnsureLoaded();
        if (!Api().export_csv) { return 4; }
        const auto utf8 = ToUtf8(outputPath);
        return Api().export_csv(WbCStringView{ utf8.data(), utf8.size() });
    }

    uint32_t PermissionRows(const wchar_t* inputPath, PermissionHandler handler)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall*)(WbCStringView, WbPermissionCallback, void*);
        const auto fn = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_permission_rows"));
        if (!fn) { return 4; }
        const auto path = ToUtf8(inputPath);
        auto callback = [](WbPermissionRow const* row, void* context) {
            (*static_cast<PermissionHandler*>(context))(*row);
        };
        return fn({ path.data(), path.size() }, callback, &handler);
    }

    WbDuplicateResult DuplicateResults(uint32_t algorithm, DuplicateHandler handler)
    {
        EnsureLoaded();
        using Fn = WbDuplicateResult(__stdcall*)(uint32_t, WbDuplicateCallback, void*);
        const auto fn = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_duplicate_results"));
        if (!fn) { return { 0, 0, 0, 4 }; }
        auto callback = [](WbDuplicateRow const* row, void* context) {
            (*static_cast<DuplicateHandler*>(context))(*row);
        };
        return fn(algorithm, callback, &handler);
    }

    uint32_t ExportAnalysis(const wchar_t* outputPath, uint32_t kind)
    {
        EnsureLoaded();using Fn=uint32_t(__stdcall*)(WbCStringView,uint32_t);
        auto fn=reinterpret_cast<Fn>(GetProcAddress(Api().module,"wb_export_analysis"));
        const auto path=ToUtf8(outputPath);return fn?fn(WbCStringView{path.data(),path.size()},kind):4;
    }
    uint32_t DumpMft(const wchar_t* rootPath, const wchar_t* outputPath)
    {
        EnsureLoaded();using Fn=uint32_t(__stdcall*)(WbCStringView,WbCStringView);
        auto fn=reinterpret_cast<Fn>(GetProcAddress(Api().module,"wb_dump_mft"));
        const auto root=ToUtf8(rootPath),path=ToUtf8(outputPath);return fn?fn(WbCStringView{root.data(),root.size()},WbCStringView{path.data(),path.size()}):4;
    }

    uint32_t ExportTsv(const wchar_t* outputPath)
    {
        EnsureLoaded();
        const auto fn = reinterpret_cast<ExportCsvFn>(GetProcAddress(Api().module, "wb_export_tsv"));
        const auto utf8 = ToUtf8(outputPath);
        return fn ? fn(WbCStringView{ utf8.data(), utf8.size() }) : 4;
    }

    uint32_t ExportJsonDetailed(const wchar_t* outputPath)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall *)(WbCStringView);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_export_json_metadata"));
        if (!function) { return 4; }
        const auto path = ToUtf8(outputPath);
        return function({path.data(), path.size()});
    }
    void WaitForSnapshotWrites()
    {
        // Only a loaded engine can have a write in flight; never load it just to exit.
        if (!Api().module) { return; }
        using Fn = void(__stdcall *)();
        if (auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_wait_for_snapshot_writes"))) {
            function();
        }
    }
    uint32_t ExportCsvDetailed(const wchar_t* outputPath)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall *)(WbCStringView);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_export_csv_metadata"));
        if (!function) { return 4; }
        const auto path = ToUtf8(outputPath);
        return function({path.data(), path.size()});
    }

    uint32_t ExportJson(const wchar_t* outputPath)
    {
        EnsureLoaded();
        if (!Api().export_json) { return 4; }
        const auto utf8 = ToUtf8(outputPath);
        return Api().export_json(WbCStringView{ utf8.data(), utf8.size() });
    }

    uint32_t ExportCsvFiltered(const wchar_t* outputPath, const wchar_t* patterns, WbCsvFilter filter)
    {
        EnsureLoaded();
        if (!Api().export_csv_filtered) { return 4; }
        const auto utf8 = ToUtf8(outputPath);
        const auto text = ToUtf8(patterns);
        filter.patterns = WbCStringView{ text.data(), text.size() };
        return Api().export_csv_filtered(WbCStringView{ utf8.data(), utf8.size() }, filter);
    }

    uint32_t ExportCsvExtension(const wchar_t* outputPath, const wchar_t* extension)
    {
        EnsureLoaded();
        if (!Api().export_csv_extension) { return 4; }
        const auto utf8 = ToUtf8(outputPath);
        const auto filter = ToUtf8(extension);
        return Api().export_csv_extension(WbCStringView{ utf8.data(), utf8.size() }, WbCStringView{ filter.data(), filter.size() });
    }

    SessionHandle StartScanWith(
        StartScanFn start_scan,
        const wchar_t* rootPath,
        EventHandler handler)
    {
        EnsureLoaded();
        const std::string utf8 = ToUtf8(rootPath);
        auto view = WbCStringView{ utf8.data(), utf8.size() };
        auto context = std::make_shared<CallbackContext>();
        context->handler = std::move(handler);

        SessionHandle session;
        session.callback_state = context;
        session.native = start_scan(view, &OnEventWithContext, context.get());
        return session;
    }

    bool ValidateScanRules(const wchar_t* exclusions, const wchar_t* includes)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall *)(WbCStringView, WbCStringView);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_validate_scan_rules"));
        if (!function) { return false; }
        const auto excluded = ToUtf8(exclusions), included = ToUtf8(includes);
        return function({excluded.data(), excluded.size()}, {included.data(), included.size()}) == 0;
    }

    SessionHandle StartScanRules(const wchar_t* rootPath, const wchar_t* exclusions,
        const wchar_t* includes, EventHandler handler)
    {
        EnsureLoaded();
        using Fn = WbScanSessionHandle(__stdcall *)(WbCStringView, WbCStringView, WbCStringView, WbEventCallback, void*);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_scan_session_start_rules"));
        if (!function) { throw winrt::hresult_error(E_NOTIMPL); }
        const auto root = ToUtf8(rootPath), exclude = ToUtf8(exclusions), include = ToUtf8(includes);
        auto context = std::make_shared<CallbackContext>();
        context->handler = std::move(handler);
        SessionHandle session;
        session.callback_state = context;
        session.native = function({root.data(), root.size()}, {exclude.data(), exclude.size()},
            {include.data(), include.size()}, &OnEventWithContext, context.get());
        if (!session.native._private) { throw winrt::hresult_error(E_INVALIDARG); }
        return session;
    }

    SessionHandle StartScan(const wchar_t* rootPath, EventHandler handler)
    {
        EnsureLoaded();
        return StartScanWith(Api().start_scan, rootPath, std::move(handler));
    }

    SessionHandle StartIncrementalScan(const wchar_t* rootPath, EventHandler handler)
    {
        EnsureLoaded();
        return StartScanWith(Api().start_incremental_scan, rootPath, std::move(handler));
    }

    void CancelScan(SessionHandle handle)
    {
        EnsureLoaded();
        Api().cancel_scan(handle.native);
    }

    void DestroyScan(SessionHandle handle)
    {
        EnsureLoaded();
        Api().destroy_scan(handle.native);
    }

    WbIndexSnapshotStats LoadCatalogSnapshotWithStats(CatalogHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<CatalogCallbackContext>();
        context->handler = std::move(handler);
        return Api().load_catalog_snapshot_with_stats(&OnCatalogWithContext, context.get());
    }

    bool LoadPublishedCatalog(CatalogHandler handler)
    {
        EnsureLoaded();
        using Fn = uint8_t(__stdcall*)(WbCatalogCallback, void*);
        static const auto fn = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_index_published_catalog"));
        auto context = std::make_shared<CatalogCallbackContext>();
        context->handler = std::move(handler);
        if (!fn) {
            Api().load_catalog_snapshot_with_stats(&OnCatalogWithContext, context.get());
            return true;
        }
        return fn(&OnCatalogWithContext, context.get()) != 0;
    }

    WbIndexSnapshotStats LoadCatalogSnapshotStats()
    {
        EnsureLoaded();
        return Api().load_catalog_snapshot_with_stats(nullptr, nullptr);
    }

    void LoadExtensionStatsSnapshot(ExtensionStatHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<ExtensionStatCallbackContext>();
        context->handler = std::move(handler);
        Api().load_extension_stats(&OnExtensionStatWithContext, context.get());
    }

    bool TreeRoot(TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        return Api().tree_root(&OnTreeNodeWithContext, context.get()) != 0;
    }

    WbMultiScanResult ScanRoots(const wchar_t* paths)
    {
        EnsureLoaded();
        using Fn = WbMultiScanResult(__stdcall *)(WbCStringView);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_scan_roots"));
        if (!function) { throw winrt::hresult_error(E_NOTIMPL); }
        const auto input = ToUtf8(paths);
        return function({input.data(), input.size()});
    }

    uint32_t ConsolidateDuplicates(const wchar_t* master, const wchar_t* duplicate)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall *)(WbCStringView, WbCStringView);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_consolidate_duplicates"));
        if (!function) { throw winrt::hresult_error(E_NOTIMPL); }
        const auto first = ToUtf8(master), second = ToUtf8(duplicate);
        return function({first.data(), first.size()}, {second.data(), second.size()});
    }

    uint32_t StorageMaintenance(const wchar_t* path, uint32_t operation)
    {
        EnsureLoaded();
        using Fn = uint32_t(__stdcall *)(WbCStringView, uint32_t);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_storage_maintenance"));
        if (!function) { throw winrt::hresult_error(E_NOTIMPL); }
        const auto input = ToUtf8(path);
        return function({input.data(), input.size()}, operation);
    }

    WbStorageEstimate StorageCost(WbStoragePolicy policy)
    {
        EnsureLoaded();
        using Fn = WbStorageEstimate(__stdcall *)(WbStoragePolicy);
        auto function = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_storage_cost"));
        if (!function) { throw winrt::hresult_error(E_NOTIMPL); }
        return function(policy);
    }

    WbSearchResult Search(const wchar_t* pattern, const wchar_t* extensions,
        WbSearchFilter filter, TreeNodeHandler handler)
    {
        EnsureLoaded();
        using Fn = WbSearchResult(__stdcall*)(WbSearchFilter, WbTreeNodeCallback, void*);
        const auto fn = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_tree_search"));
        if (!fn) { return { 0, 0, 4 }; }
        const auto text = ToUtf8(pattern);
        const auto ext = ToUtf8(extensions);
        filter.pattern = { text.data(), text.size() };
        filter.extensions = { ext.data(), ext.size() };
        TreeNodeCallbackContext context{ std::move(handler) };
        return fn(filter, &OnTreeNodeWithContext, &context);
    }

    WbTreeChildrenResult TreeChildren(uint64_t parentId, uint64_t offset, TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        return Api().tree_children(parentId, offset, &OnTreeNodeWithContext, context.get());
    }

    bool LastRefreshTouched(std::vector<uint64_t>& ids)
    {
        EnsureLoaded();
        using Fn = int64_t(__stdcall*)(uint64_t*, size_t);
        static const auto fn = reinterpret_cast<Fn>(GetProcAddress(Api().module, "wb_last_refresh_touched"));
        ids.clear();
        if (!fn) { return false; }
        const int64_t count = fn(nullptr, 0);
        if (count < 0) { return false; }
        ids.resize(static_cast<size_t>(count));
        const int64_t written = fn(ids.data(), ids.size());
        if (written != count) { return false; }
        return true;
    }

    WbTreeChildrenResult TreeLargestFilesPage(uint64_t offset, uint64_t limit, TreeNodeHandler handler)
    {
        EnsureLoaded();
        if (!Api().tree_largest_files_page) { return {}; }
        TreeNodeCallbackContext context{ std::move(handler) };
        return Api().tree_largest_files_page(offset, limit, &OnTreeNodeWithContext, &context);
    }

    void TreeLargestFiles(uint64_t limit, TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        Api().tree_largest_files(limit, &OnTreeNodeWithContext, context.get());
    }

    WbUpdateCheck CheckForUpdate(const std::string& currentVersion, const std::string& responseJson)
    {
        EnsureLoaded();
        WbUpdateCheck result{};
        if (Api().update_check == nullptr) {
            return result;  // older DLL without the export; treat as "unknown".
        }
        const WbCStringView current{ currentVersion.c_str(), currentVersion.size() };
        const WbCStringView json{ responseJson.c_str(), responseJson.size() };
        return Api().update_check(current, json);
    }

    uint8_t VerifyDownload(const std::string& manifestJson, const std::string& kind, const std::string& actualHash)
    {
        EnsureLoaded();
        if (Api().verify_download == nullptr) {
            return 2; // older DLL without the export: indeterminate (proceed on transport trust).
        }
        const WbCStringView m{ manifestJson.c_str(), manifestJson.size() };
        const WbCStringView k{ kind.c_str(), kind.size() };
        const WbCStringView h{ actualHash.c_str(), actualHash.size() };
        return Api().verify_download(m, k, h);
    }
}
