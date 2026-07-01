//! Little-endian byte codec shared by the index (de)serializers (M26 page persistence). Hand-rolled (no serde
//! dep) and bounds-checked on read — a corrupt index page surfaces as a typed `Err`, never a panic/UB.
//! Used by both `ivf.rs` (flat lists) and `hnsw.rs` (a 3-level neighbour graph).

pub(crate) fn put_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}
pub(crate) fn put_u64(b: &mut Vec<u8>, n: u64) {
    b.extend_from_slice(&n.to_le_bytes());
}
pub(crate) fn put_i64(b: &mut Vec<u8>, n: i64) {
    b.extend_from_slice(&n.to_le_bytes());
}
pub(crate) fn put_f32(b: &mut Vec<u8>, x: f32) {
    b.extend_from_slice(&x.to_le_bytes());
}
pub(crate) fn put_vec_f32(b: &mut Vec<u8>, v: &[f32]) {
    put_u32(b, v.len() as u32);
    for x in v {
        put_f32(b, *x);
    }
}
pub(crate) fn put_vecs_f32(b: &mut Vec<u8>, vs: &[Vec<f32>]) {
    put_u32(b, vs.len() as u32);
    for v in vs {
        put_vec_f32(b, v);
    }
}
pub(crate) fn put_vec_usize(b: &mut Vec<u8>, v: &[usize]) {
    put_u32(b, v.len() as u32);
    for x in v {
        put_u64(b, *x as u64);
    }
}
pub(crate) fn put_vecs_usize(b: &mut Vec<u8>, vs: &[Vec<usize>]) {
    put_u32(b, vs.len() as u32);
    for v in vs {
        put_vec_usize(b, v);
    }
}
pub(crate) fn put_vecs_vecs_usize(b: &mut Vec<u8>, vs: &[Vec<Vec<usize>>]) {
    put_u32(b, vs.len() as u32);
    for v in vs {
        put_vecs_usize(b, v);
    }
}

/// A bounds-checked little-endian read cursor. Every getter returns `Err` on a short read (never panics).
pub(crate) struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cur<'a> {
    pub(crate) fn new(b: &'a [u8]) -> Self {
        Cur { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.p.checked_add(n).ok_or("theodb wire: length overflow")?;
        if end > self.b.len() {
            return Err("theodb wire: truncated blob".into());
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    /// Reserve capacity for `count` elements of `elem_bytes` each WITHOUT trusting `count`: a corrupt length field
    /// must not trigger a multi-GB allocation before the per-element `take()` bounds check fires. Caps at the
    /// bytes actually remaining (the real read still `Err`s if the blob is short).
    fn capacity_for(&self, count: usize, elem_bytes: usize) -> usize {
        let remaining = self.b.len().saturating_sub(self.p);
        count.min(remaining / elem_bytes.max(1))
    }
    pub(crate) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub(crate) fn usize(&mut self) -> Result<usize, String> {
        Ok(self.u64()? as usize)
    }
    pub(crate) fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub(crate) fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn vec_f32(&mut self) -> Result<Vec<f32>, String> {
        let n = self.u32()? as usize;
        let mut v = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            v.push(self.f32()?);
        }
        Ok(v)
    }
    pub(crate) fn vecs_f32(&mut self) -> Result<Vec<Vec<f32>>, String> {
        let n = self.u32()? as usize;
        let mut out = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            out.push(self.vec_f32()?);
        }
        Ok(out)
    }
    pub(crate) fn vec_usize(&mut self) -> Result<Vec<usize>, String> {
        let n = self.u32()? as usize;
        let mut v = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            v.push(self.usize()?);
        }
        Ok(v)
    }
    pub(crate) fn vecs_usize(&mut self) -> Result<Vec<Vec<usize>>, String> {
        let n = self.u32()? as usize;
        let mut out = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            out.push(self.vec_usize()?);
        }
        Ok(out)
    }
    pub(crate) fn vecs_vecs_usize(&mut self) -> Result<Vec<Vec<Vec<usize>>>, String> {
        let n = self.u32()? as usize;
        let mut out = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            out.push(self.vecs_usize()?);
        }
        Ok(out)
    }
    pub(crate) fn i64_vec(&mut self) -> Result<Vec<i64>, String> {
        let n = self.u32()? as usize;
        let mut v = Vec::with_capacity(self.capacity_for(n, 4));
        for _ in 0..n {
            v.push(self.i64()?);
        }
        Ok(v)
    }
}
