fn main() {
    // `sqlx::migrate!("./migrations")` embeds every file in this
    // directory at compile time. Without this, cargo has no reason to
    // know that *adding* a new migration file (as opposed to editing one
    // it's already tracking) should trigger a rebuild -- a stale
    // binary silently ships without the new migration, exactly the
    // failure mode this line exists to prevent.
    println!("cargo:rerun-if-changed=migrations");
}
