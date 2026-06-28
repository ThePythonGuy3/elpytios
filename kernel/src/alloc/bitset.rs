#[repr(transparent)]
pub struct AllocBitset([u32]);
impl AllocBitset {
    #[inline]
    pub const fn size_for(bits: usize) -> usize {
        bits.div_ceil(u32::BITS as usize)
    }
}
