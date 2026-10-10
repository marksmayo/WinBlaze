#pragma once
#include <atomic>
#include <exception>
#include <functional>
#include <mutex>
#include <memory>
#include <stdexcept>
#include <thread>
#include <utility>
#include <vector>

// The owner must drain before releasing data captured by its jobs.
// Only a job already owned by this group can add work after closure.
class OwnedBackgroundWork
{
public:
    ~OwnedBackgroundWork() { try { CloseAndDrain(); } catch (...) {} }
    bool Accepting()
    {
        std::lock_guard guard(m_mutex);
        return m_accepting;
    }
    bool Run(std::function<void()> job)
    {
        std::lock_guard guard(m_mutex);
        if (!m_accepting && s_current != this) return false;
        ReapLocked();
        auto finished = std::make_shared<std::atomic<bool>>(false);
        // Allocate the slot before starting: allocation failure cannot abandon
        // a joinable thread or unregistered job.
        m_threads.emplace_back();
        try {
        m_threads.back().finished = finished;
        m_threads.back().thread = std::thread([this, finished, job = std::move(job)] {
            auto previous = s_current;
            s_current = this;
            try { job(); }
            catch (...) {
                std::lock_guard guard(m_mutex);
                if (!m_error) m_error = std::current_exception();
            }
            s_current = previous;
            finished->store(true);
        });
        } catch (...) { m_threads.pop_back(); throw; }
        return true;
    }
    void ReapCompleted()
    {
        std::lock_guard guard(m_mutex);
        ReapLocked();
    }
    size_t RetainedJobs()
    {
        std::lock_guard guard(m_mutex);
        return m_threads.size();
    }
    void CloseAndDrain()
    {
        if (s_current == this) throw std::logic_error("owned job cannot drain itself");
        std::lock_guard drain_guard(m_drain_mutex);
        for (;;) {
            std::vector<Job> batch;
            {
                std::lock_guard guard(m_mutex);
                m_accepting = false;
                batch.swap(m_threads);
            }
            if (batch.empty()) break;
            for (auto& job : batch) job.thread.join();
        }
        std::exception_ptr error;
        {
            std::lock_guard guard(m_mutex);
            error = std::exchange(m_error, {});
        }
        if (error) std::rethrow_exception(error);
    }
private:
    struct Job {
        std::thread thread;
        std::shared_ptr<std::atomic<bool>> finished;
    };
    void ReapLocked()
    {
        for (auto iterator = m_threads.begin(); iterator != m_threads.end();) {
            if (iterator->finished->load()) {
                // Completed jobs no longer touch the group or its mutex.
                iterator->thread.join();
                iterator = m_threads.erase(iterator);
            } else ++iterator;
        }
    }
    inline static thread_local OwnedBackgroundWork* s_current{};
    std::mutex m_mutex;
    std::mutex m_drain_mutex;
    bool m_accepting{true};
    std::vector<Job> m_threads;
    std::exception_ptr m_error;
};
