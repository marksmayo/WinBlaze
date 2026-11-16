#pragma once

#include <cstddef>
#include <cstdint>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

namespace WinBlaze::UI::TreePath
{
    // An absolute Windows path needs a root directory, which is always a
    // separator, so a segment without one can skip the std::filesystem::path
    // construction (and its allocation) entirely.
    inline bool MayBeAbsolute(std::wstring_view segment)
    {
        return segment.find_first_of(L"\\/") != std::wstring_view::npos;
    }

    // Joins the names from the root down to `node_index`, where each node has
    // `name` (std::wstring) and `parent` (SIZE_MAX at a root). An absolute
    // segment restarts the path. One chain buffer and one result buffer sized
    // up front: no per-segment allocation.
    template <class Nodes>
    std::wstring Build(Nodes const& nodes, size_t node_index)
    {
        size_t depth = 0;
        size_t capacity = 0;
        for (size_t current = node_index; current != SIZE_MAX && current < nodes.size();
             current = nodes[current].parent) {
            ++depth;
            capacity += nodes[current].name.size() + 1;
        }
        std::wstring path;
        if (depth == 0) {
            return path;
        }
        std::vector<size_t> chain;
        chain.reserve(depth);
        for (size_t current = node_index; current != SIZE_MAX && current < nodes.size();
             current = nodes[current].parent) {
            chain.push_back(current);
        }
        path.reserve(capacity);
        for (auto it = chain.rbegin(); it != chain.rend(); ++it) {
            auto const& segment = nodes[*it].name;
            if (path.empty() ||
                (MayBeAbsolute(segment) && std::filesystem::path(segment).is_absolute())) {
                path.assign(segment);
            } else {
                if (path.back() != L'\\') {
                    path += L'\\';
                }
                path += segment;
            }
        }
        return path;
    }
}
