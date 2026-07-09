#[repr(transparent)]
pub struct AllocBitset([u32]);
impl AllocBitset {
    #[inline]
    pub const fn size_for(bits: usize) -> usize {
        bits.div_ceil(u32::BITS as usize)
    }

    #[inline]
    pub unsafe fn get_and_toggle(&mut self, bit: u32) -> bool {
        let block_index = bit / u32::BITS;
        let block_bit = 1 << (bit & (u32::BITS - 1));

        let block = unsafe { self.0.get_unchecked_mut(block_index as usize) };
        let old_block = *block;

        *block = old_block ^ block_bit;
        old_block & block_bit != 0
    }
}
