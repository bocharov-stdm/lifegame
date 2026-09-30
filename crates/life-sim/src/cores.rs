//! Keeping the thread that drives the tick on the processor's fast cores. On a hybrid processor
//! (performance and efficiency cores) Windows moved it onto a slow core while the pool's threads
//! worked on the decisions, and the tick's one-thread phases went half as slow again: on an
//! i7-13650HX a ×100 world took 12.5 ms a tick on every thread and 8.6 ms with the process held on
//! the fast cores. Only the driving thread is held: the pool keeps every core. Elsewhere, and on a
//! processor with one kind of core, nothing happens.

/// Keep the calling thread on the processor's fastest cores; whether it was done.
pub fn pin_to_fast_cores() -> bool {
    imp::pin()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::SystemInformation::{
        CpuSetInformation, GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
    };
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadSelectedCpuSets};

    pub fn pin() -> bool {
        let fast = fast_cores();
        // SAFETY: the pseudo-handle of the calling thread is always valid; the ids come from the system
        !fast.is_empty() && unsafe { SetThreadSelectedCpuSets(GetCurrentThread(), &fast) }.as_bool()
    }

    /// The CPU sets of the highest efficiency class (the fastest cores); none when all are alike.
    fn fast_cores() -> Vec<u32> {
        let mut len = 0u32;
        // SAFETY: a null buffer only asks for the length
        let _ = unsafe { GetSystemCpuSetInformation(None, 0, &mut len, None, None) };
        if len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: the buffer holds `len` bytes, aligned for the entries' u64 fields
        let filled = unsafe {
            GetSystemCpuSetInformation(Some(buffer.as_mut_ptr().cast()), len, &mut len, None, None)
        };
        if !filled.as_bool() {
            return Vec::new();
        }
        let bytes = buffer.as_ptr().cast::<u8>();
        let entry = size_of::<SYSTEM_CPU_SET_INFORMATION>();
        let mut sets: Vec<(u8, u32)> = Vec::new();
        let mut at = 0usize;
        while at + entry <= len as usize {
            // SAFETY: within the filled bytes; entries may lie unaligned, so read as such
            let info = unsafe { bytes.add(at).cast::<SYSTEM_CPU_SET_INFORMATION>().read_unaligned() };
            if info.Size == 0 {
                break;
            }
            if info.Type == CpuSetInformation {
                // SAFETY: an entry of this type holds a CPU set
                let set = unsafe { info.Anonymous.CpuSet };
                sets.push((set.EfficiencyClass, set.Id));
            }
            at += info.Size as usize;
        }
        let top = sets.iter().map(|s| s.0).max();
        if top.is_none() || top == sets.iter().map(|s| s.0).min() {
            return Vec::new();
        }
        sets.into_iter().filter(|s| Some(s.0) == top).map(|s| s.1).collect()
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn pin() -> bool {
        false
    }
}
