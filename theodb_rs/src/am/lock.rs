//! Transaction-scoped advisory lock keyed on the index OID (M26 — serialize the VACUUM blob-rewrite against
//! concurrent scans/inserts). VACUUM's `ShareUpdateExclusiveLock` on the table does NOT block SELECT/INSERT, and
//! `rewrite_blob` re-maps the whole page layout non-atomically — so without this a concurrent scan could read a
//! torn blob and a concurrent insert could be lost. Scans/inserts take SHARE; the VACUUM fold takes EXCLUSIVE.
use pgrx::pg_sys;

unsafe fn acquire(oid: pg_sys::Oid, mode: pg_sys::LOCKMODE) {
    unsafe {
        let mut tag: pg_sys::LOCKTAG = std::mem::zeroed();
        tag.locktag_field1 = pg_sys::MyDatabaseId.to_u32();
        tag.locktag_field2 = oid.to_u32();
        tag.locktag_field3 = 0;
        tag.locktag_field4 = 1;
        tag.locktag_type = pg_sys::LockTagType::LOCKTAG_ADVISORY as u8;
        tag.locktag_lockmethodid = pg_sys::USER_LOCKMETHOD as u8;
        // sessionLock=false → released at transaction end; dontWait=false → block until granted.
        pg_sys::LockAcquire(&tag, mode, false, false);
    }
}

/// SHARE the index-fold lock — compatible with other scans/inserts, blocks a concurrent VACUUM rewrite.
pub(crate) fn index_shared(rel: pg_sys::Relation) {
    unsafe { acquire((*rel).rd_id, pg_sys::ShareLock as pg_sys::LOCKMODE) }
}

/// EXCLUSIVE the index-fold lock — the VACUUM rewrite waits for all readers/writers, then runs alone.
pub(crate) fn index_exclusive(rel: pg_sys::Relation) {
    unsafe { acquire((*rel).rd_id, pg_sys::ExclusiveLock as pg_sys::LOCKMODE) }
}
