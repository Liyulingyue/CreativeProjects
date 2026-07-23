pub const KV_BLOCK_SIZE: usize = 16;
pub const MAX_HEAD_DIM: usize = 128;
pub const MAX_KV_BLOCKS: usize = 4096;

#[derive(Clone)]
#[repr(C)]
pub struct PagedKVBlock {
    pub k: [[f32; MAX_HEAD_DIM]; KV_BLOCK_SIZE],
    pub v: [[f32; MAX_HEAD_DIM]; KV_BLOCK_SIZE],
    pub ref_count: u16,
    pub seq_id: u16,
    pub pos_start: u32,
    pub valid_tokens: u8,
}

impl PagedKVBlock {
    pub const fn new() -> Self {
        Self {
            k: [[0.0f32; MAX_HEAD_DIM]; KV_BLOCK_SIZE],
            v: [[0.0f32; MAX_HEAD_DIM]; KV_BLOCK_SIZE],
            ref_count: 0,
            seq_id: 0,
            pos_start: 0,
            valid_tokens: 0,
        }
    }

    pub fn reset(&mut self) {
        self.ref_count = 0;
        self.seq_id = 0;
        self.pos_start = 0;
        self.valid_tokens = 0;
    }

    pub fn is_free(&self) -> bool {
        self.ref_count == 0
    }
}

pub struct BlockAllocator {
    blocks: Box<[PagedKVBlock]>,
    free_stack: Box<[u16]>,
    free_top: u16,
    capacity: u16,
}

impl BlockAllocator {
    pub fn new(capacity: u16) -> Self {
        let blocks = vec![PagedKVBlock::new(); capacity as usize].into_boxed_slice();
        let mut free_stack = vec![0u16; capacity as usize].into_boxed_slice();
        for i in 0..capacity {
            free_stack[i as usize] = i;
        }
        Self {
            blocks,
            free_stack,
            free_top: capacity,
            capacity,
        }
    }

    pub fn alloc(&mut self) -> Option<u16> {
        if self.free_top == 0 {
            return None;
        }
        self.free_top -= 1;
        let idx = self.free_stack[self.free_top as usize];
        self.blocks[idx as usize].ref_count = 1;
        Some(idx)
    }

    pub fn free(&mut self, idx: u16) -> bool {
        let block = &mut self.blocks[idx as usize];
        if block.ref_count == 0 {
            return false;
        }
        block.ref_count -= 1;
        if block.ref_count == 0 {
            block.reset();
            self.free_stack[self.free_top as usize] = idx;
            self.free_top += 1;
        }
        true
    }

    pub fn get(&self, idx: u16) -> Option<&PagedKVBlock> {
        if (idx as usize) < self.blocks.len() {
            Some(&self.blocks[idx as usize])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, idx: u16) -> Option<&mut PagedKVBlock> {
        if (idx as usize) < self.blocks.len() {
            Some(&mut self.blocks[idx as usize])
        } else {
            None
        }
    }

    pub fn used_count(&self) -> u16 {
        self.capacity - self.free_top
    }

    pub fn capacity(&self) -> u16 {
        self.capacity
    }
}

pub struct KVCacheView {
    pub head_dim: usize,
    pub n_heads_kv: usize,
    pub layer_idx: u32,
    pub block_indices: Box<[u16]>,
    pub num_blocks: usize,
}

impl KVCacheView {
    pub fn new(head_dim: usize, n_heads_kv: usize, max_blocks: usize) -> Self {
        Self {
            head_dim,
            n_heads_kv,
            layer_idx: 0,
            block_indices: vec![0u16; max_blocks].into_boxed_slice(),
            num_blocks: 0,
        }
    }

    pub fn total_tokens(&self) -> usize {
        self.num_blocks * KV_BLOCK_SIZE
    }

    pub fn push_block(&mut self, idx: u16) -> bool {
        if self.num_blocks >= self.block_indices.len() {
            return false;
        }
        self.block_indices[self.num_blocks] = idx;
        self.num_blocks += 1;
        true
    }
}

pub struct MemoryArena {
    data: Box<[u8]>,
    pub scratch_offset: usize,
    pub kv_offset: usize,
    pub total_size: usize,
}

impl MemoryArena {
    pub fn new(scratch_bytes: usize, kv_bytes: usize) -> Self {
        let total_size = scratch_bytes + kv_bytes;
        let data = vec![0u8; total_size].into_boxed_slice();
        Self {
            data,
            scratch_offset: 0,
            kv_offset: scratch_bytes,
            total_size,
        }
    }

    pub fn scratch_slice(&mut self) -> &mut [f32] {
        let start = self.scratch_offset;
        let end = self.kv_offset;
        let byte_len = end - start;
        let f32_len = byte_len / core::mem::size_of::<f32>();
        let ptr = self.data[start..].as_mut_ptr() as *mut f32;
        unsafe { core::slice::from_raw_parts_mut(ptr, f32_len) }
    }

    pub fn kv_slice(&mut self) -> &mut [u8] {
        &mut self.data[self.kv_offset..]
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }
}
