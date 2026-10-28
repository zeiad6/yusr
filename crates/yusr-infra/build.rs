// `sqlx::migrate!` embeds the migrations at compile time; without this, adding a migration
// file leaves incremental builds running the old set.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
