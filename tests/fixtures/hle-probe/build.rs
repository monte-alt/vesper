fn main() {
    println!("cargo:rustc-link-arg-cdylib=/DELAYLOAD:vesper-probe-delayed.dll");
    println!("cargo:rustc-link-lib=delayimp");
}
