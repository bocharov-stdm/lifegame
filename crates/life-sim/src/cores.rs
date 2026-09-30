//! Keeping the thread that drives the tick on the processor's fast cores. On a hybrid processor
//! (performance and efficiency cores) Windows moved it onto a slow core while the pool's threads
//! worked on the decisions, and the tick's one-thread phases went half as slow again: on an
//! i7-13650HX a ×100 world took 12.5 ms a tick on every thread and 8.6 ms with the process held on
//! the fast cores. Only the driving thread is held: the pool keeps every core. Elsewhere, and on a
//! processor with one kind of core, nothing happens.

/// Keep the calling thread on the processor's fastest cores, or (`on` false) let it go anywhere
/// again; whether it was done.
pub fn keep_on_fast_cores(on: bool) -> bool {
    imp::keep(on)
}

/// The processor has cores of more than one kind, so keeping to the fast ones means something.
pub fn has_fast_cores() -> bool {
    imp::has_fast_cores()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::SystemInformation::{
        CpuSetInformation, GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
    };
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadSelectedCpuSets};

    pub fn keep(on: bool) -> bool {
        let fast = fast_cores();
        if fast.is_empty() {
            return false;
        }
        // no ids: the thread's own choice of cores is dropped and it goes wherever the process may
        let ids: &[u32] = if on { &fast } else { &[] };
        // SAFETY: the pseudo-handle of the calling thread is always valid; the ids come from the system
        unsafe { SetThreadSelectedCpuSets(GetCurrentThread(), ids) }.as_bool()
    }

    pub fn has_fast_cores() -> bool {
        !fast_cores().is_empty()
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
    pub fn keep(_: bool) -> bool {
        false
    }

    pub fn has_fast_cores() -> bool {
        false
    }
}

#[cfg(all(test, windows))]
mod tests {
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadSelectedCpuSets};

    /// How many CPU sets the calling thread keeps to (0: none of its own).
    fn held() -> u32 {
        let mut count = 0u32;
        // SAFETY: an empty buffer only asks for the count
        let _ = unsafe { GetThreadSelectedCpuSets(GetCurrentThread(), None, &mut count) };
        count
    }

    /// Held, the thread keeps to the fast cores; let go, to none of its own.
    #[test]
    fn a_thread_keeps_to_the_fast_cores_and_lets_go() {
        if !super::has_fast_cores() {
            return; // one kind of core: nothing to keep to
        }
        assert!(super::keep_on_fast_cores(true));
        assert!(held() > 0, "held on the fast cores");
        assert!(super::keep_on_fast_cores(false));
        assert_eq!(held(), 0, "let go");
    }
}
