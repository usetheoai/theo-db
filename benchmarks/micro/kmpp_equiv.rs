// M34 regression proof (committed, reproducible): the incremental k-means++ init (O(k·n·d)) produces
// BYTE-IDENTICAL centroids to the original recompute-min init (O(k²·n·d)) — same RNG order, same selections.
// Guards the ivf.rs kmeanspp equivalence invariant. Standalone: rustc -O --edition 2021 kmpp_equiv.rs && ./kmpp_equiv

// Prove the incremental k-means++ init produces IDENTICAL centers to the original recompute-min version
// (same RNG order, same selections). Small data; both use the exact SplitMix64 + f32 l2 shape.
struct Rng(u64);
impl Rng { fn new(s:u64)->Self{Rng(s)} fn next_u64(&mut self)->u64{self.0=self.0.wrapping_add(0x9E3779B97F4A7C15);let mut z=self.0;z=(z^(z>>30)).wrapping_mul(0xBF58476D1CE4E5B9);z=(z^(z>>27)).wrapping_mul(0x94D049BB133111EB);z^(z>>31)} fn next_f64(&mut self)->f64{let v=(self.next_u64()>>11)as f64/((1u64<<53)as f64);if v<=0.0{f64::MIN_POSITIVE}else{v}} }
fn l2(a:&[f32],b:&[f32])->f64{let mut s=0f32;for i in 0..a.len(){let d=a[i]-b[i];s+=d*d;}(s as f64).sqrt()}
fn old(vs:&[Vec<f32>],k:usize,seed:u64)->Vec<Vec<f32>>{
    let mut rng=Rng::new(seed);let n=vs.len();let mut c=vec![vs[(rng.next_u64() as usize)%n].clone()];
    while c.len()<k{
        let d2:Vec<f64>=vs.iter().map(|v|c.iter().map(|x|{let d=l2(v,x);d*d}).fold(f64::INFINITY,f64::min)).collect();
        let sum:f64=d2.iter().sum(); if sum<=0.0{c.push(vs[c.len()%n].clone());continue;}
        let mut t=rng.next_f64()*sum;let mut ch=0;for(i,w)in d2.iter().enumerate(){t-=*w;if t<=0.0{ch=i;break;}}
        c.push(vs[ch].clone());
    } c
}
fn new_(vs:&[Vec<f32>],k:usize,seed:u64)->Vec<Vec<f32>>{
    let mut rng=Rng::new(seed);let n=vs.len();let first=vs[(rng.next_u64() as usize)%n].clone();
    let mut d2:Vec<f64>=vs.iter().map(|v|{let d=l2(v,&first);d*d}).collect();let mut c=vec![first];
    while c.len()<k{
        let sum:f64=d2.iter().sum();
        let ch=if sum<=0.0{c.len()%n}else{let mut t=rng.next_f64()*sum;let mut x=0;for(i,w)in d2.iter().enumerate(){t-=*w;if t<=0.0{x=i;break;}}x};
        let ct=vs[ch].clone();
        for(i,v)in vs.iter().enumerate(){let d=l2(v,&ct);let dd=d*d;if dd<d2[i]{d2[i]=dd;}}
        c.push(ct);
    } c
}
fn main(){
    let mut rng=Rng::new(99);
    let vs:Vec<Vec<f32>>=(0..500).map(|_|(0..16).map(|_|rng.next_f64() as f32).collect()).collect();
    for k in [1usize,5,20,50,100]{
        let a=old(&vs,k,42); let b=new_(&vs,k,42);
        let same=a.len()==b.len() && a.iter().zip(&b).all(|(x,y)|x==y);
        println!("k={k}: identical={same}");
        assert!(same,"k={k} centroids diverge");
    }
    println!("ALL IDENTICAL — the O(k) init is byte-for-byte equivalent to the O(k^2) original");
}
