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
    // An empty file is still one (empty) part, matching what the web client sends.
    let num_chunks = ((size as f64 / chunk_size as f64).ceil() as usize).max(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_chunk_boundary() {
        let chunks = build_chunks_array(25, 10);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[2].offset, 20);
        assert_eq!(chunks[2].size, 5);
    }

    #[test]
    fn exact_multiple_has_no_trailing_chunk() {
        let chunks = build_chunks_array(20, 10);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].size, 10);
    }

    #[test]
    fn empty_file_is_still_one_part() {
        let chunks = build_chunks_array(0, 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].size, 0);
    }
}
