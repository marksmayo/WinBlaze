#pragma once

#include <chrono>
#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "winblaze_native.h"

namespace WinBlaze::UI::NativeBridge
{
    using EventHandler = std::function<void(const WbEvent&)>;
    using CatalogHandler = std::function<void(const WbCatalogEntry&)>;
    using ExtensionStatHandler = std::function<void(const WbExtensionStat&)>;
    using PermissionHandler = std::function<void(const WbPermissionRow&)>;
    using DuplicateHandler = std::function<void(const WbDuplicateRow&)>;
    using TreeNodeHandler = std::function<void(const WbTreeNode&)>;

    struct SessionHandle
    {
        WbScanSessionHandle native{};
        std::shared_ptr<void> callback_state{};
    };

    void Initialize();
    uint32_t ImportCsv(const wchar_t* inputPath, bool wiztreeUtc = false);
    // Begins importing a saved scan on a worker thread, once per launch. The
    // next ImportCsv call for the same file returns that import's result.
    void PrestartImportCsv(const wchar_t* inputPath, bool wiztreeUtc);
    // Prestarts the saved scan named as the only command-line argument, if any.
    void PrestartCommandLineImport();
    // Waits up to `limit` for the prestarted import to finish. False at once
    // when nothing was prestarted, or when the import is still running.
    bool WaitForPrestartedImport(std::chrono::milliseconds limit);
    // Native imports actually run, and imports begun ahead of their request.
    uint32_t ImportRunCount();
    uint32_t ImportPrestartCount();
    uint32_t ExportCsv(const wchar_t* outputPath);
    uint32_t PermissionRows(const wchar_t* path, PermissionHandler handler);
    WbDuplicateResult DuplicateResults(uint32_t algorithm, DuplicateHandler handler);
    uint32_t ExportAnalysis(const wchar_t* outputPath, uint32_t kind);
    uint32_t DumpMft(const wchar_t* rootPath, const wchar_t* outputPath);
    uint32_t ExportTsv(const wchar_t* outputPath);
    uint32_t ExportJson(const wchar_t* outputPath);
    uint32_t ExportJsonDetailed(const wchar_t* outputPath);
    uint32_t ExportCsvDetailed(const wchar_t* outputPath);
    // Blocks until no catalog snapshot write is in flight (no-op before the engine loads).
    void WaitForSnapshotWrites();
    uint32_t ExportCsvFiltered(const wchar_t* outputPath, const wchar_t* patterns, WbCsvFilter filter);
    uint32_t ExportCsvExtension(const wchar_t* outputPath, const wchar_t* extension);
    bool ValidateScanRules(const wchar_t* exclusions, const wchar_t* includes);
    SessionHandle StartScanRules(const wchar_t* rootPath, const wchar_t* exclusions, const wchar_t* includes, EventHandler handler);
    SessionHandle StartScan(const wchar_t* rootPath, EventHandler handler);
    SessionHandle StartIncrementalScan(const wchar_t* rootPath, EventHandler handler);
    void CancelScan(SessionHandle handle);
    void DestroyScan(SessionHandle handle);
    WbIndexSnapshotStats LoadCatalogSnapshotWithStats(CatalogHandler handler);
    // The catalog's counts only: loads the index model, emits no entries.
    WbIndexSnapshotStats LoadCatalogSnapshotStats();
    void LoadExtensionStatsSnapshot(ExtensionStatHandler handler);
    // Emits the display-tree root; returns false when no index is loaded.
    bool TreeRoot(TreeNodeHandler handler);
    // Emits direct children of a directory (largest physical first, capped,
    // starting at `offset`); `total` reports how many exist so callers can
    // page and show a "+N more" row.
    WbMultiScanResult ScanRoots(const wchar_t* paths);
    uint32_t ConsolidateDuplicates(const wchar_t* master, const wchar_t* duplicate);
    uint32_t StorageMaintenance(const wchar_t* path, uint32_t operation);
    WbStorageEstimate StorageCost(WbStoragePolicy policy);
    WbSearchResult Search(const wchar_t* pattern, const wchar_t* extensions, WbSearchFilter filter, TreeNodeHandler handler);
    WbTreeChildrenResult TreeChildren(uint64_t parentId, uint64_t offset, TreeNodeHandler handler);
    // Directory ids whose totals the last refresh patched in place; false
    // when the published results were replaced since (show them anew).
    bool LastRefreshTouched(std::vector<uint64_t>& ids);
    // Emits the largest files by allocation size (name = full path).
    WbTreeChildrenResult TreeLargestFilesPage(uint64_t offset, uint64_t limit, TreeNodeHandler handler);
    void TreeLargestFiles(uint64_t limit, TreeNodeHandler handler);
    // Compares currentVersion against a GitHub releases/latest JSON body.
    WbUpdateCheck CheckForUpdate(const std::string& currentVersion, const std::string& responseJson);
    // Verifies a downloaded artifact's SHA-256 against an update manifest:
    // 1 = match, 0 = mismatch (abort), 2 = no entry / can't verify.
    uint8_t VerifyDownload(const std::string& manifestJson, const std::string& kind, const std::string& actualHash);
}
