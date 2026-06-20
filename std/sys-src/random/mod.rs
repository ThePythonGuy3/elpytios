pub fn fill_bytes(_: &mut [u8]) {
    panic!("this target does not support random data generation");
}

pub fn hashmap_random_keys() -> (u64, u64) {
    let mut buf = [0; 16];
    fill_bytes(&mut buf);
    let k1 = u64::from_ne_bytes(buf[..8].try_into().unwrap());
    let k2 = u64::from_ne_bytes(buf[8..].try_into().unwrap());
    (k1, k2)
}
