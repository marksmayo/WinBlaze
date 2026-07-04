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
}
