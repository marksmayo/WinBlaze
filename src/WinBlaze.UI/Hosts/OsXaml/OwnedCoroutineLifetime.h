#pragma once
#include <chrono>
#include <condition_variable>
#include <functional>
#include <memory>
#include <mutex>

// Drain on the UI thread while the framework and complete window remain live.
// Every admitted operation retains a lease through its final UI continuation.
class OwnedCoroutineLifetime {
public:
    std::shared_ptr<void> Begin() {
        std::lock_guard guard(m_mutex);
        if (!m_accepting) return {};
        // Allocate before registration: failed allocation creates no lease.
        auto lease = std::make_shared<Lease>();
        lease->owner = this;
        ++m_active;
        return lease;
    }
    void CloseAndDrain(std::function<void()> pump) {
        std::unique_lock guard(m_mutex);
        m_accepting = false;
        while (m_active) {
            guard.unlock();
            pump();
            guard.lock();
            if (m_active) m_changed.wait_for(guard, std::chrono::milliseconds(1));
        }
    }
private:
    struct Lease {
        OwnedCoroutineLifetime* owner{};
        ~Lease() {
            if (!owner) return;
            std::lock_guard guard(owner->m_mutex);
            --owner->m_active;
            owner->m_changed.notify_all();
        }
    };
    std::mutex m_mutex;
    std::condition_variable m_changed;
    bool m_accepting{true};
    size_t m_active{};
};
