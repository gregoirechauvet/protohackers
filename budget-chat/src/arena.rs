#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Index {
    pub idx: u32,
    pub version: u32,
}

impl Index {
    /// Packs the index and version into a 64-bit integer for io_uring
    pub fn into_user_data(self) -> u64 {
        ((self.version as u64) << 32) | (self.idx as u64)
    }

    /// Unpacks the 64-bit integer returned by the kernel
    pub fn from_user_data(user_data: u64) -> Self {
        Self {
            version: (user_data >> 32) as u32,
            idx: (user_data & 0xFFFFFFFF) as u32,
        }
    }
}

enum State<T> {
    /// A vacant slot pointing to the next free index in the arena.
    Free { next_free: Option<u32> },
    /// An active operation owning the data and buffers.
    Occupied(T),
}

struct Entry<T> {
    /// The current version of this slot.
    /// Increments every time the slot is freed.
    version: u32,
    state: State<T>,
}

pub struct Arena<T> {
    entries: Vec<Entry<T>>,
    free_head: Option<u32>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_head: None,
        }
    }

    pub fn insert(&mut self, value: T) -> Index {
        match self.free_head {
            Some(idx) => {
                // Reuse an existing free slot
                let entry = &mut self.entries[idx as usize];

                // Update the free_head to point to the next free slot
                if let State::Free { next_free } = entry.state {
                    self.free_head = next_free;
                } else {
                    unreachable!("free_head pointed to an occupied slot");
                }

                // Occupy the slot. The version remains what it was.
                entry.state = State::Occupied(value);

                Index {
                    idx,
                    version: entry.version,
                }
            }
            None => {
                // No free slots available, push a new one
                let idx = self.entries.len() as u32;
                let version = 0;

                self.entries.push(Entry {
                    version,
                    state: State::Occupied(value),
                });

                Index { idx, version }
            }
        }
    }

    pub fn remove(&mut self, index: Index) -> Option<T> {
        let entry = self.entries.get_mut(index.idx as usize)?;

        // The ABA guard: if the version doesn't match, this is a stale index.
        if entry.version != index.version {
            return None;
        }

        // We use std::mem::replace to extract the Occupied state
        // and immediately replace it with a Free state.
        let state = std::mem::replace(
            &mut entry.state,
            State::Free {
                next_free: self.free_head,
            },
        );

        match state {
            State::Occupied(value) => {
                // Bump the version so stale indices will fail the check next time.
                // wrapping_add prevents panics if a slot is reused 4 billion times.
                entry.version = entry.version.wrapping_add(1);

                // Update the free list to point to this newly freed slot.
                self.free_head = Some(index.idx);

                Some(value)
            }
            State::Free { .. } => None,
        }
    }

    pub fn get(&self, index: Index) -> Option<&T> {
        let entry = self.entries.get(index.idx as usize)?;

        if entry.version != index.version {
            return None;
        }
        
        match &entry.state {
            State::Occupied(value) => Some(value),
            State::Free { .. } => None,
        }
    }

    pub fn get_mut(&mut self, index: Index) -> Option<&mut T> {
        let entry = self.entries.get_mut(index.idx as usize)?;

        if entry.version != index.version {
            return None;
        }

        match &mut entry.state {
            State::Occupied(value) => Some(value),
            State::Free { .. } => None,
        }
    }
}
