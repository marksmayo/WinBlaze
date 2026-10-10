#pragma once
#include "OwnedBackgroundWork.h"
#include <coroutine>

// Resume on the owned worker. A caller must explicitly marshal back to UI,
// and abandon that continuation when its window is closing.
class OwnedBackgroundAwait {
public:
    OwnedBackgroundAwait(OwnedBackgroundWork& owner, std::function<void()> job)
        : m_owner(owner), m_job(std::move(job)) {}
    bool await_ready() const noexcept { return false; }
    bool await_suspend(std::coroutine_handle<> continuation) {
        if (!m_owner.Run([this, continuation] {
            try { m_job(); } catch (...) { m_error = std::current_exception(); }
            continuation.resume();
            // resume can destroy this awaiter; never access it afterward.
        })) throw std::logic_error("background coroutine owner is closed");
        return true;
    }
    void await_resume() {
        if (m_error) std::rethrow_exception(m_error);
    }
private:
    OwnedBackgroundWork& m_owner;
    std::function<void()> m_job;
    std::exception_ptr m_error;
};
