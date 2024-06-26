use std::ops::Range;
#[cfg(feature = "pack")]
use std::{
    io::{Read, Result, Write},
    path::Path,
};

#[cfg(feature = "pack")]
use super::packed_sa::PackedSuffixArray;
use super::saca::saca;
use super::utils::*;

/// Suffix array for byte string.
#[derive(Clone)]
pub struct SuffixArray {
    s: Vec<u8>,
    sa: Vec<u32>,
    bkt: Option<Vec<u32>>,
}

fn binary_search<T, F>(xs: &[T], mut pred: F) -> usize
where
    F: FnMut(&T) -> bool,
{
    let (mut left, mut right) = (0, xs.len());
    while left < right {
        let mid = (left + right) / 2;
        if pred(&xs[mid]) {
            right = mid;
        } else {
            left = mid + 1;
        }
    }
    left
}

impl SuffixArray {
    // Construct new suffix array for given byte string.
    pub fn new(s: Vec<u8>) -> Self {
        let mut sa = vec![0; s.len() + 1];
        saca(&s, &mut sa[..]);
        SuffixArray { s, sa, bkt: None }
    }

    // Construct suffix array in place.
    pub fn set(&mut self, s: &[u8]) {
        self.sa.resize(s.len() + 1, 0);
        saca(s, &mut self.sa[..]);
    }

    // Release the unused memory of suffix array.
    pub fn fit(&mut self) {
        self.sa.shrink_to_fit()
    }

    /// Length of the underlying byte string.
    pub fn len(&self) -> usize {
        self.s.len()
    }

    /// Test if the underlying byte string is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_parts(&self) -> (&[u8], &[u32]) {
        (&self.s, &self.sa)
    }

    /// Take out the suffix array and its corresponding byte string.
    pub fn into_parts(self) -> (Vec<u8>, Vec<u32>) {
        (self.s, self.sa)
    }

    /// Compose existed suffix array and its corresponding byte string
    /// together, and checks the integrity.
    pub fn from_parts(s: Vec<u8>, sa: Vec<u32>) -> Option<Self> {
        let compose = SuffixArray { s, sa, bkt: None };
        if compose.check_integrity() {
            Some(compose)
        } else {
            None
        }
    }

    /// Compose existed suffix array and its corresponding byte string
    /// together without integrity check.
    pub unsafe fn unchecked_from_parts(s: Vec<u8>, sa: Vec<u32>) -> Self {
        SuffixArray { s, sa, bkt: None }
    }

    fn check_integrity(&self) -> bool {
        if self.s.len() + 1 != self.sa.len() {
            return false;
        }
        for i in 1..self.sa.len() {
            let x = &self.s[self.sa[i - 1] as usize..];
            let y = &self.s[self.sa[i] as usize..];
            if x >= y {
                return false;
            }
        }
        true
    }

    /// Enable bucket pointers to speed up large amount of pattern searching.
    ///
    /// The overhead is about 257 KiB.
    pub fn enable_buckets(&mut self) {
        if self.bkt.is_some() {
            return;
        }

        // the layout is [$; (0, $), (0, 0), ..., (0, 255); ...; (255, $), (255, 0), ..., (255, 255)]
        let mut bkt = vec![0; 256 * 257 + 1];

        // count occurrences.
        bkt[0] = 1;
        if self.s.len() > 0 {
            for i in 0..self.s.len() - 1 {
                let c0 = unsafe { *self.s.get_unchecked(i) };
                let c1 = unsafe { *self.s.get_unchecked(i + 1) };
                let idx = (c0 as usize * 257) + (c1 as usize + 1) + 1;
                bkt[idx] += 1;
            }
            let c0 = unsafe { *self.s.get_unchecked(self.s.len() - 1) };
            let idx = (c0 as usize * 257) + 1;
            bkt[idx] += 1;
        }

        // store the right boundaries of each bucket.
        let mut sum = 0;
        for p in bkt.iter_mut() {
            sum += *p;
            *p = sum;
        }

        self.bkt = Some(bkt);
    }

    /// Get the bucket of pattern.
    #[inline]
    fn get_bucket(&self, pat: &[u8]) -> Range<usize> {
        if let Some(ref bkt) = self.bkt {
            if pat.len() > 1 {
                // sub-bucket (c0, c1).
                let c0 = pat[0];
                let c1 = pat[1];
                let idx = (c0 as usize * 257) + (c1 as usize + 1) + 1;
                bkt[idx - 1] as usize..bkt[idx] as usize
            } else if pat.len() == 1 {
                // top-level bucket (c0, $)..=(c0, 255).
                let c0 = pat[0];
                let start_idx = c0 as usize * 257;
                let end_idx = start_idx + 257;
                bkt[start_idx] as usize..bkt[end_idx] as usize
            } else {
                // the sentinel bucket.
                0..1
            }
        } else {
            0..self.sa.len()
        }
    }

    /// Get the top-level bucket.
    #[inline]
    fn get_top_bucket(&self, pat: &[u8]) -> Range<usize> {
        if let Some(ref bkt) = self.bkt {
            if pat.len() > 0 {
                let c0 = pat[0];
                let start_idx = c0 as usize * 257;
                let end_idx = start_idx + 257;
                bkt[start_idx] as usize..bkt[end_idx] as usize
            } else {
                0..1
            }
        } else {
            0..self.sa.len()
        }
    }

    pub fn find_region(&self, pat: &[u8]) -> (usize, usize) {
        let bucket = self.get_bucket(pat);

        let s = &self.s;
        let sa = &self.sa[bucket.clone()];

        let start = binary_search(&sa, |&i| pat <= &s[i as usize..]);
        let end = start + binary_search(&sa[start..], |&i| {
            !s[i as usize..].starts_with(pat)
        });

        (bucket.start + start, bucket.start + end)
    }

    /// Test if it contains the given pattern.
    pub fn contains(&self, pat: &[u8]) -> bool {
        let s = &self.s;
        let sa = &self.sa[self.get_bucket(pat)];

        sa.binary_search_by_key(&pat, |&i| trunc(&s[i as usize..], pat.len()))
            .is_ok()
    }

    /// Search for all the unsorted occurrence of given pattern (can overlap).
    pub fn search_all(&self, pat: &[u8]) -> &[u32] {
        let s = &self.s;
        let sa = if pat.len() > 0 {
            &self.sa[self.get_bucket(pat)]
        } else {
            &self.sa[..]
        };

        let mut i = 0;
        let mut k = sa.len();
        while i < k {
            let m = i + (k - i) / 2;
            if pat > &s[sa[m] as usize..] {
                i = m + 1;
            } else {
                k = m;
            }
        }

        let mut j = i;
        let mut k = sa.len();
        while j < k {
            let m = j + (k - j) / 2;
            if s[sa[m] as usize..].starts_with(pat) {
                j = m + 1;
            } else {
                k = m;
            }
        }

        &sa[i..j]
    }

    /// Search for a sub-string that has the longest common prefix of the given pattern.
    pub fn search_lcp(&self, pat: &[u8]) -> Range<usize> {
        let s = &self.s;
        let sa = &self.sa[self.get_bucket(pat)];

        if sa.len() == 0 {
            // pat.len() > 0, for any i < s.len(): lcp(pat, s[i..]) <= 1.
            let sa = &self.sa[self.get_top_bucket(pat)];
            if sa.len() > 0 {
                // there exists i < s.len(): lcp(pat, s[i..]) == 1.
                let i = sa[0] as usize;
                return i..i + 1;
            } else {
                // for any i < s.len(): lcp(pat, s[i..]) == 0.
                return self.s.len()..self.s.len();
            }
        }

        match sa.binary_search_by(|&i| s[i as usize..].cmp(pat)) {
            Ok(i) => {
                // find a suffix equals to the pattern.
                let start = sa[i] as usize;
                start..s.len()
            }
            Err(i) => {
                // find a position to insert the pattern.
                if i > 0 && i < sa.len() {
                    let start_a = sa[i - 1] as usize;
                    let start_b = sa[i] as usize;
                    let len_a = lcp(pat, &s[start_a..]);
                    let len_b = lcp(pat, &s[start_b..]);
                    if len_a > len_b {
                        start_a..start_a + len_a
                    } else {
                        start_b..start_b + len_b
                    }
                } else if i == 0 {
                    let start = sa[i] as usize;
                    let len = lcp(pat, &s[start..]);
                    start..start + len
                } else {
                    let start = sa[i - 1] as usize;
                    let len = lcp(pat, &s[start..]);
                    start..start + len
                }
            }
        }
    }

    /// Dump the suffix array.
    #[cfg(feature = "pack")]
    pub fn dump<W: Write>(&self, file: W) -> Result<()> {
        let psa = PackedSuffixArray::from_sa(&self.sa[..]);
        psa.dump(file)
    }

    /// Create a file and write the suffix array.
    #[cfg(feature = "pack")]
    pub fn dump_file<P: AsRef<Path>>(&self, name: P) -> Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let file = BufWriter::new(File::create(name)?);
        let psa = PackedSuffixArray::from_sa(&self.sa[..]);
        psa.dump(file)
    }

    /// Dump the suffix array as bytes.
    #[cfg(feature = "pack")]
    pub fn dump_bytes(&self) -> Result<Vec<u8>> {
        let psa = PackedSuffixArray::from_sa(&self.sa[..]);
        psa.dump_bytes()
    }

    /// Load suffix array from reader without integrity check.
    #[cfg(feature = "pack")]
    pub unsafe fn unchecked_load<R: Read>(
        s: &'a [u8],
        file: R,
    ) -> Result<Self> {
        let psa = PackedSuffixArray::load(file)?;
        let sa = psa.into_sa();
        Ok(Self::unchecked_from_parts(s, sa))
    }

    /// Load suffix array from reader.
    #[cfg(feature = "pack")]
    pub fn load<R: Read>(s: &'a [u8], file: R) -> Result<Self> {
        use std::io::{Error, ErrorKind};

        let sa = unsafe { Self::unchecked_load(s, file)? };
        if !sa.check_integrity() {
            Err(Error::new(
                ErrorKind::InvalidData,
                "inconsistent suffix array",
            ))
        } else {
            Ok(sa)
        }
    }

    /// Load suffix array from a file without integrity check.
    #[cfg(feature = "pack")]
    pub unsafe fn unchecked_load_file<P: AsRef<Path>>(
        s: &'a [u8],
        name: P,
    ) -> Result<Self> {
        use std::fs::File;
        use std::io::BufReader;

        let file = BufReader::new(File::open(name)?);
        Self::unchecked_load(s, file)
    }

    /// Load suffix array from a file.
    #[cfg(feature = "pack")]
    pub fn load_file<P: AsRef<Path>>(s: &'a [u8], name: P) -> Result<Self> {
        use std::io::{Error, ErrorKind};

        let sa = unsafe { Self::unchecked_load_file(s, name)? };
        if !sa.check_integrity() {
            Err(Error::new(
                ErrorKind::InvalidData,
                "inconsistent suffix array",
            ))
        } else {
            Ok(sa)
        }
    }

    /// Load suffix array from bytes without integrity check.
    #[cfg(feature = "pack")]
    pub unsafe fn unchecked_load_bytes(
        s: &'a [u8],
        bytes: &[u8],
    ) -> Result<Self> {
        let psa = PackedSuffixArray::load_bytes(bytes)?;
        let sa = psa.into_sa();
        Ok(Self::unchecked_from_parts(s, sa))
    }

    /// Load suffix array from bytes.
    #[cfg(feature = "pack")]
    pub fn load_bytes(s: &'a [u8], bytes: &[u8]) -> Result<Self> {
        use std::io::{Error, ErrorKind};

        let sa = unsafe { Self::unchecked_load_bytes(s, bytes)? };
        if !sa.check_integrity() {
            Err(Error::new(
                ErrorKind::InvalidData,
                "inconsistent suffix array",
            ))
        } else {
            Ok(sa)
        }
    }
}

impl From<SuffixArray> for Vec<u32> {
    fn from(sa: SuffixArray) -> Vec<u32> {
        sa.sa
    }
}

impl AsRef<[u8]> for SuffixArray {
    fn as_ref(&self) -> &[u8] {
        &self.s
    }
}
