fn main() {
    #[cfg(feature = "qm-native")]
    {
        println!("cargo:rerun-if-changed=vendor/qm-dsp");

        cc::Build::new()
            .cpp(true)
            .std("c++11")
            .include("vendor/qm-dsp")
            // TempoTrackV2
            .file("vendor/qm-dsp/dsp/tempotracking/TempoTrackV2.cpp")
            // Maths utilities
            .file("vendor/qm-dsp/maths/MathUtilities.cpp")
            // C wrapper
            .file("vendor/qm-dsp/wrapper.cpp")
            // Compiler flags for warnings
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-sign-compare")
            .compile("qm-dsp");
    }
}
