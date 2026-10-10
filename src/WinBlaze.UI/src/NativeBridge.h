#pragma once

#include <functional>
#include <memory>
#include <string>

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
    uint32_t ExportCsv(const wchar_t* outputPath);
    uint32_t PermissionRows(const wchar_t* path, PermissionHandler handler);
    WbDuplicateResult DuplicateResults(uint32_t algorithm, DuplicateHandler handler);
    uint32_t ExportAnalysis(const wchar_t* outputPath, uint32_t kind);
    uint32_t DumpMft(const wchar_t* rootPath, const wchar_t* outputPath);
    uint32_t ExportTsv(const wchar_t* outputPath);
    uint32_t ExportJson(const wchar_t* outputPath);
    uint32_t ExportJsonDetailed(const wchar_t* outputPath);
    uint32_t ExportCsvDetailed(const wchar_t* outputPath);
    uint32_t ExportCsvFiltered(const wchar_t* outputPath, const wchar_t* patterns, WbCsvFilter filter);
    uint32_t ExportCsvExtension(const wchar_t* outputPath, const wchar_t* extension);
    bool ValidateScanRules(const wchar_t* exclusions, const wchar_t* includes);
    SessionHandle StartScanRules(const wchar_t* rootPath, const wchar_t* exclusions, const wchar_t* includes, EventHandler handler);
    SessionHandle StartScan(const wchar_t* rootPath, EventHandler handler);
    SessionHandle StartIncrementalScan(const wchar_t* rootPath, EventHandler handler);
    void CancelScan(SessionHandle handle);
    void DestroyScan(SessionHandle handle);
    WbIndexSnapshotStats LoadCatalogSnapshotWithStats(CatalogHandler handler);
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
    // Emits the largest files by allocation size (name = full path).
    WbTreeChildrenResult TreeLargestFilesPage(uint64_t offset, uint64_t limit, TreeNodeHandler handler);
    void TreeLargestFiles(uint64_t limit, TreeNodeHandler handler);
    // Compares currentVersion against a GitHub releases/latest JSON body.
    WbUpdateCheck CheckForUpdate(const std::string& currentVersion, const std::string& responseJson);
    // Verifies a downloaded artifact's SHA-256 against an update manifest:
    // 1 = match, 0 = mismatch (abort), 2 = no entry / can't verify.
    uint8_t VerifyDownload(const std::string& manifestJson, const std::string& kind, const std::string& actualHash);
}
