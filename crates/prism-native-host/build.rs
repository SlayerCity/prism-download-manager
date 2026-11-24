//! Windows (MSVC): link the VC runtime statically and the Universal CRT dynamically.
//! Without this, the native host needs vcruntime140.dll and browsers cannot start it
//! on systems without the Visual C++ Redistributable (#111).
fn main() {
    static_vcruntime::metabuild();
}
