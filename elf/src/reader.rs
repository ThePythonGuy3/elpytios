use bytemuck::AnyBitPattern;

#[derive(Clone)]
pub struct Reader<'a> {
    bytes: &'a [u8],
}

pub enum ReaderError {
    Eof,
}

impl Reader<'_> {
    pub fn advance(&mut self, count: usize) -> Result<&[u8], ReaderError> {
        self.bytes.split_at(5);
        todo!()
    }

    pub fn read<T: AnyBitPattern>(&mut self) -> Result<T, ReaderError> {
        todo!()
    }
}
