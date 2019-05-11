use super::sais_ints_mut;
use proptest::prelude::*;

proptest!{
    #[test]
    fn sais_ints_mut_correctness(mut s in ints(1..8192_usize)) {
        prop_assert!(check(&mut s[..]));
    }
}

fn ints(scale: impl Strategy<Value=usize>) -> impl Strategy<Value=Vec<u32>> {
    scale.prop_flat_map(|k| {
        prop::collection::vec(0..k as u32, k..k+1)
    })
}

fn check(s: &mut [u32]) -> bool {
    let k = s.len();
    let mut sa = vec![0; s.len() + 1];
    sais_ints_mut(s, k, &mut sa[..]);
    for i in 1..sa.len() {
        let x = &s[sa[i - 1] as usize..];
        let y = &s[sa[i] as usize..];
        if x >= y {
            return false;
        }
    }
    true
}
