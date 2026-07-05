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
    using TreeNodeHandler = std::function<void(const WbTreeNode&)>;

    struct SessionHandle
    {
        WbScanSessionHandle native{};
        std::shared_ptr<void> callback_state{};
    };

    void Initialize();
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
    WbTreeChildrenResult TreeChildren(uint64_t parentId, uint64_t offset, TreeNodeHandler handler);
    // Emits the largest files by allocation size (name = full path).
    void TreeLargestFiles(uint64_t limit, TreeNodeHandler handler);
}
