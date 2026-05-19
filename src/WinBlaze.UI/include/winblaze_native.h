#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct WbCStringView
{
    const char* ptr;
    size_t len;
} WbCStringView;

typedef struct WbNativeError
{
    uint32_t code;
    WbCStringView message;
} WbNativeError;

typedef struct WbCatalogEntry
{
    WbCStringView name;
    WbCStringView path;
    WbCStringView kind;
    WbCStringView size_text;
    WbCStringView description;
    uint64_t size_bytes;
    int64_t modified_utc;
    uint8_t has_modified_utc;
} WbCatalogEntry;

typedef void(__stdcall* WbCatalogCallback)(const WbCatalogEntry* entry, void* user_data);

typedef struct WbIndexSnapshotStats
{
    uint64_t volumes;
    uint64_t directories;
    uint64_t files;
    uint64_t entries_emitted_limit;
    uint64_t cache_read_bytes;
    uint64_t cache_read_millis;
    uint64_t cache_decode_millis;
    uint8_t cache_loaded_from_backup;
} WbIndexSnapshotStats;

typedef enum WbEventKind
{
    WbEventKind_SessionStarted = 1,
    WbEventKind_Progress = 2,
    WbEventKind_Summary = 3,
    WbEventKind_Completed = 4,
    WbEventKind_Cancelled = 5,
    WbEventKind_Failed = 6,
    WbEventKind_Issue = 7,
    WbEventKind_VolumeDiscovered = 8,
    WbEventKind_DirectoryFound = 9,
    WbEventKind_FileFound = 10,
    WbEventKind_IncrementalChanges = 11,
} WbEventKind;

typedef struct WbScanSummary
{
    uint64_t files_seen;
    uint64_t directories_seen;
    uint64_t total_size_bytes;
    uint64_t total_allocation_bytes;
} WbScanSummary;

typedef struct WbIncrementalChangeSummary
{
    uint64_t added;
    uint64_t removed;
    uint64_t modified;
    uint64_t renamed;
    uint64_t moved;
} WbIncrementalChangeSummary;

typedef struct WbEvent
{
    WbEventKind kind;
    uint64_t progress_items_done;
    uint64_t progress_items_total;
    uint64_t progress_bytes_done;
    uint64_t progress_bytes_total;
    WbScanSummary summary;
    WbIncrementalChangeSummary incremental_changes;
    WbNativeError error;
    WbCatalogEntry catalog_entry;
} WbEvent;

typedef struct WbScanSessionHandle
{
    void* _private;
} WbScanSessionHandle;

typedef void(__stdcall* WbEventCallback)(const WbEvent* event, void* user_data);

WbScanSessionHandle wb_scan_session_start(WbCStringView root_path, WbEventCallback callback, void* user_data);
WbScanSessionHandle wb_scan_session_start_incremental(WbCStringView root_path, WbEventCallback callback, void* user_data);
void wb_scan_session_cancel(WbScanSessionHandle handle);
void wb_scan_session_destroy(WbScanSessionHandle handle);
void wb_index_snapshot_load(WbCatalogCallback callback, void* user_data);
WbIndexSnapshotStats wb_index_snapshot_load_with_stats(WbCatalogCallback callback, void* user_data);
WbIndexSnapshotStats wb_index_snapshot_stats(void);

#ifdef __cplusplus
}
#endif
