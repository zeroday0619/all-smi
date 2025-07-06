use std::io::Write;

pub struct BufferWriter {
    buffer: String,
}

impl BufferWriter {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(1024 * 1024), // Pre-allocate 1MB
        }
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))?;
        self.buffer.push_str(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
