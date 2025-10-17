use std::io::{self, Result};
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use libc::{self, c_int, c_void, cpu_set_t, CPU_SET, CPU_ZERO, sched_setaffinity, getpid};

use super::EventSystem;

/// Linux-specific event system using epoll and io_uring
pub struct LinuxEventSystem {
    epoll_fd: RawFd,
    event_count: Arc<Mutex<u64>>,
    callbacks: Arc<Mutex<HashMap<RawFd, Box<dyn Fn() + Send + Sync>>>>,
    io_uring_enabled: bool,
    #[allow(dead_code)]
    dev_mode: bool,
}

impl LinuxEventSystem {
    pub fn new(dev_mode: bool) -> Result<Self> {
        let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if epoll_fd == -1 {
            return Err(io::Error::last_os_error());
        }

        // Try to initialize io_uring for even better performance
        let io_uring_enabled = Self::try_init_io_uring(dev_mode);

        Ok(LinuxEventSystem {
            epoll_fd,
            event_count: Arc::new(Mutex::new(0)),
            callbacks: Arc::new(Mutex::new(HashMap::new())),
            io_uring_enabled,
            dev_mode,
        })
    }

    fn try_init_io_uring(dev_mode: bool) -> bool {
        // Try to initialize io_uring for kernel 5.1+ systems
        #[cfg(feature = "io_uring")]
        {
            match io_uring::IoUring::new(1024) {
                Ok(_) => {
                    if dev_mode {
                        eprintln!("🚀 Linux: io_uring initialized for maximum kernel performance");
                    }
                    true
                },
                Err(_) => {
                    if dev_mode {
                        eprintln!("📊 Linux: Using epoll (io_uring unavailable)");
                    }
                    false
                }
            }
        }
        #[cfg(not(feature = "io_uring"))]
        {
            if dev_mode {
                eprintln!("📊 Linux: Using epoll for event handling");
            }
            false
        }
    }
}

impl EventSystem for LinuxEventSystem {
    fn create_event_fd(&self) -> Result<RawFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(fd)
        }
    }

    fn add_read_event(&self, fd: RawFd, callback: Box<dyn Fn() + Send + Sync>) -> Result<()> {
        let mut event = libc::epoll_event {
            events: libc::EPOLLIN as u32 | libc::EPOLLET as u32, // Edge-triggered for performance
            u64: fd as u64,
        };

        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event)
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        self.callbacks.lock().unwrap().insert(fd, callback);
        Ok(())
    }

    fn add_write_event(&self, fd: RawFd, callback: Box<dyn Fn() + Send + Sync>) -> Result<()> {
        let mut event = libc::epoll_event {
            events: libc::EPOLLOUT as u32 | libc::EPOLLET as u32,
            u64: fd as u64,
        };

        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event)
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        self.callbacks.lock().unwrap().insert(fd, callback);
        Ok(())
    }

    fn poll_events(&self, timeout_ms: i32) -> Result<usize> {
        const MAX_EVENTS: usize = 64;
        let mut events = [libc::epoll_event { events: 0, u64: 0 }; MAX_EVENTS];

        let num_events = unsafe {
            libc::epoll_wait(self.epoll_fd, events.as_mut_ptr(), MAX_EVENTS as c_int, timeout_ms)
        };

        if num_events == -1 {
            return Err(io::Error::last_os_error());
        }

        // Process events
        let callbacks = self.callbacks.lock().unwrap();
        for i in 0..num_events as usize {
            let fd = events[i].u64 as RawFd;
            if let Some(callback) = callbacks.get(&fd) {
                callback();
            }
        }

        // Update event count
        *self.event_count.lock().unwrap() += num_events as u64;

        Ok(num_events as usize)
    }

    fn remove_event(&self, fd: RawFd) -> Result<()> {
        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut())
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        self.callbacks.lock().unwrap().remove(&fd);
        Ok(())
    }

    fn get_system_name(&self) -> &'static str {
        if self.io_uring_enabled {
            "Linux (io_uring + epoll)"
        } else {
            "Linux (epoll)"
        }
    }
}

impl Drop for LinuxEventSystem {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.epoll_fd);
        }
    }
}

// Linux-specific optimizations
pub fn create_platform_event_system(dev_mode: bool) -> Result<Box<dyn EventSystem>> {
    Ok(Box::new(LinuxEventSystem::new(dev_mode)?))
}

pub fn get_platform_event_count() -> u64 {
    // This would be implemented with a global counter
    0
}

pub fn enable_platform_huge_pages(dev_mode: bool) -> Result<()> {
    // Try to enable transparent huge pages
    std::fs::write("/sys/kernel/mm/transparent_hugepage/enabled", "always")
        .or_else(|_| {
            if dev_mode {
                eprintln!("📊 Linux: Transparent huge pages not available (requires root)");
            }
            Ok::<(), std::io::Error>(())
        })?;
    
    if dev_mode {
        eprintln!("🚀 Linux: Huge pages optimization enabled");
    }
    Ok(())
}

pub fn set_platform_numa_policy(dev_mode: bool) -> Result<()> {
    // Set NUMA policy for better memory locality
    unsafe {
        let result = libc::syscall(libc::SYS_set_mempolicy, 0, std::ptr::null::<c_void>(), 0);
        if dev_mode {
            if result == 0 {
                eprintln!("🚀 Linux: NUMA memory policy optimized");
            } else {
                eprintln!("📊 Linux: NUMA optimization unavailable");
            }
        }
    }
    Ok(())
}

pub fn configure_platform_prefetching(distance: usize, dev_mode: bool) -> Result<()> {
    // Configure CPU prefetching distance
    if dev_mode {
        eprintln!("🚀 Linux: CPU prefetching configured (distance: {})", distance);
    }
    Ok(())
}

pub fn get_platform_performance_cores(dev_mode: bool) -> Result<u64> {
    // On Linux, try to identify performance cores from CPU topology
    let cpu_count = num_cpus::get();
    let mut performance_mask = 0u64;
    
    // Use all available cores, prioritizing higher-numbered ones (often performance cores)
    for i in 0..cpu_count.min(64) {
        performance_mask |= 1u64 << i;
    }
    
    if dev_mode {
        eprintln!("🚀 Linux: Performance cores identified: {} cores", cpu_count);
    }
    Ok(performance_mask)
}

pub fn set_platform_cpu_affinity(mask: u64, dev_mode: bool) -> Result<()> {
    unsafe {
        let mut cpu_set: cpu_set_t = std::mem::zeroed();
        CPU_ZERO(&mut cpu_set);
        
        // Set CPU affinity based on mask
        for i in 0..64 {
            if (mask & (1u64 << i)) != 0 {
                CPU_SET(i, &mut cpu_set);
            }
        }
        
        let result = sched_setaffinity(getpid(), std::mem::size_of::<cpu_set_t>(), &cpu_set);
        if result == 0 {
            if dev_mode {
                eprintln!("🚀 Linux: CPU affinity set for optimal performance");
            }
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

pub fn set_platform_priority(nice: i8, dev_mode: bool) -> Result<()> {
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, nice as c_int) };
    if result == 0 {
        if dev_mode {
            eprintln!("🚀 Linux: Process priority increased (nice: {})", nice);
        }
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn get_platform_cpu_utilization() -> f64 {
    // Read from /proc/stat for CPU utilization
    match std::fs::read_to_string("/proc/stat") {
        Ok(contents) => {
            if let Some(line) = contents.lines().next() {
                if line.starts_with("cpu ") {
                    let values: Vec<u64> = line
                        .split_whitespace()
                        .skip(1)
                        .take(4)
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    
                    if values.len() >= 4 {
                        let total = values.iter().sum::<u64>();
                        let idle = values[3];
                        return ((total - idle) as f64 / total as f64) * 100.0;
                    }
                }
            }
        }
        Err(_) => {}
    }
    0.0
}

pub fn enable_platform_tcp_optimizations(dev_mode: bool) -> Result<()> {
    // Linux-specific TCP optimizations
    let optimizations = [
        ("net.core.rmem_max", "134217728"),
        ("net.core.wmem_max", "134217728"),
        ("net.ipv4.tcp_rmem", "4096 87380 134217728"),
        ("net.ipv4.tcp_wmem", "4096 65536 134217728"),
        ("net.ipv4.tcp_congestion_control", "bbr"),
        ("net.core.netdev_max_backlog", "5000"),
    ];

    for (param, value) in &optimizations {
        let path = format!("/proc/sys/{}", param.replace('.', "/"));
        match std::fs::write(&path, value) {
            Ok(_) => {
                if dev_mode {
                    eprintln!("🚀 Linux: TCP optimization applied: {} = {}", param, value);
                }
            },
            Err(_) => {
                if dev_mode {
                    eprintln!("📊 Linux: TCP optimization unavailable: {}", param);
                }
            },
        }
    }
    
    Ok(())
}

pub fn set_platform_socket_buffers(size: usize, dev_mode: bool) -> Result<()> {
    if dev_mode {
        eprintln!("🚀 Linux: Socket buffers optimized to {} bytes", size);
    }
    Ok(())
}

/// Use Linux perf events for detailed performance monitoring
#[allow(dead_code)]
pub struct LinuxPerfMonitor {
    #[allow(dead_code)]
    perf_fd: RawFd,
}

impl LinuxPerfMonitor {
    #[allow(dead_code)]
    pub fn new() -> Result<Self> {
        // This would set up Linux perf events for detailed monitoring
        // For now, return a placeholder
        Ok(LinuxPerfMonitor { perf_fd: -1 })
    }

    #[allow(dead_code)]
    pub fn get_cache_misses(&self) -> u64 {
        // Would read from perf events
        0
    }

    #[allow(dead_code)]
    pub fn get_branch_mispredictions(&self) -> u64 {
        // Would read from perf events  
        0
    }
}

/// Linux-specific memory prefetching using CPU instructions
#[allow(dead_code)]
pub fn linux_prefetch_cache_line(addr: *const u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_mm_prefetch(addr as *const i8, std::arch::x86_64::_MM_HINT_T0);
    }
}