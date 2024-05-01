#[derive(Debug, Clone)]
pub struct Chunk {
    pub index: usize,
    pub offset: usize,
    pub size: usize,
}

impl Chunk {
    pub fn new(index: usize, offset: usize, size: usize) -> Self {
        Self {
            index,
            offset,
            size,
        }
    }
}

pub fn build_chunks_array(size: usize, chunk_size: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let num_chunks = (size as f64 / chunk_size as f64).ceil() as usize;
    for i in 0..num_chunks {
        let offset = i * chunk_size;
        let size = if offset + chunk_size <= size {
            chunk_size
        } else {
            size - offset
        };
        chunks.push(Chunk::new(i, offset, size));
    }
    chunks
}
