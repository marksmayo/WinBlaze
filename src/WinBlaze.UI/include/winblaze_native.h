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
    /* Record id and parent directory id (valid when has_parent is set), so
       live scan events can be assembled into a tree. File and directory id
       counters overlap numerically. */
    uint64_t id;
    uint64_t parent_id;
    uint8_t has_parent;
    uint8_t is_directory;
    uint64_t size_bytes;
    /* Physical (on-disk allocation) size. For directories/volumes this is
       the same rolled-up value as size_bytes (no separate logical-size
       rollup is tracked for directories yet). */
    uint64_t allocation_bytes;
    uint64_t total_entries;
    int64_t modified_utc;
    uint8_t has_modified_utc;
} WbCatalogEntry;

typedef void(__stdcall* WbCatalogCallback)(const WbCatalogEntry* entry, void* user_data);

typedef struct WbExtensionStat
{
    WbCStringView extension;
    WbCStringView description;
    uint64_t bytes;
    uint64_t files;
} WbExtensionStat;

typedef struct WbExtensionStatsSnapshot
{
    const WbExtensionStat* items;
    size_t count;
} WbExtensionStatsSnapshot;

typedef void(__stdcall* WbExtensionStatCallback)(const WbExtensionStat* entry, void* user_data);

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
    WbEventKind_ExtensionStats = 12,
    WbEventKind_DirectoryBatch = 13,
} WbEventKind;

/* One directory in a live scan batch. */
typedef struct WbLiveDirectory
{
    uint64_t id;
    uint64_t parent_id;
    uint8_t has_parent;
    WbCStringView name;
} WbLiveDirectory;

/* Borrowed view over a batch of scan-discovered directories; valid only for
   the duration of the callback invocation. Directories arrive batched
   because a full drive emits hundreds of thousands. */
typedef struct WbLiveDirectoryBatch
{
    const WbLiveDirectory* items;
    size_t count;
} WbLiveDirectoryBatch;

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
    WbExtensionStatsSnapshot extension_stats;
    WbLiveDirectoryBatch directory_batch;
} WbEvent;

/* One entry in the display tree. `id` identifies a directory only when
   is_directory is set - file and directory id counters overlap numerically,
   so file ids must not be passed back to wb_tree_children. The name view is
   valid only for the duration of the callback. */
typedef struct WbTreeNode
{
    uint64_t id;
    uint8_t is_directory;
    WbCStringView name;
    uint64_t logical_bytes;
    uint64_t physical_bytes;
    uint64_t file_count;
    uint64_t item_count;
    int64_t modified_utc;
    uint8_t has_modified_utc;
} WbTreeNode;

typedef struct WbTreeChildrenResult
{
    uint64_t emitted;
    uint64_t total;
} WbTreeChildrenResult;

typedef void(__stdcall* WbTreeNodeCallback)(const WbTreeNode* node, void* user_data);

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
void wb_index_snapshot_extension_stats(WbExtensionStatCallback callback, void* user_data);
/* Emits the display-tree root; returns 1 when a root exists, 0 for an empty
   index. The root node's name is its full mount-point path. */
uint8_t wb_tree_root(WbTreeNodeCallback callback, void* user_data);
/* Emits direct children of directory parent_id, largest physical size first,
   capped at 4096 starting at offset; total lets callers page and render a
   "+N more" row. */
WbTreeChildrenResult wb_tree_children(uint64_t parent_id, uint64_t offset, WbTreeNodeCallback callback, void* user_data);
/* Emits the largest files by allocation size, descending; node.name carries
   the full display path. */
void wb_tree_largest_files(uint64_t limit, WbTreeNodeCallback callback, void* user_data);

typedef struct WbUpdateCheck
{
    uint8_t available;   /* 1 when `latest` is newer than the current version */
    uint8_t parsed;      /* 1 when a tag was parsed from the response */
    uint8_t latest_len;  /* bytes used in `latest` */
    uint8_t latest[32];  /* latest tag, UTF-8, not NUL-terminated */
} WbUpdateCheck;

/* Compares current_version against a GitHub releases/latest JSON body and
   reports whether a newer release is available. The caller does the fetch. */
WbUpdateCheck wb_update_check(WbCStringView current_version, WbCStringView response_json);

#ifdef __cplusplus
}
#endif
