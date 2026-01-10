/* C wrapper implementation for QM-DSP TempoTrackV2 */

#include "wrapper.h"
#include "dsp/tempotracking/TempoTrackV2.h"

#include <vector>

struct QmTempoTracker {
    TempoTrackV2* impl;
    float sample_rate;
    int df_increment;
};

extern "C" {

QmTempoTracker* qm_tempo_new(float sample_rate, int df_increment) {
    QmTempoTracker* tracker = new (std::nothrow) QmTempoTracker;
    if (!tracker) {
        return nullptr;
    }

    tracker->impl = new (std::nothrow) TempoTrackV2(sample_rate, df_increment);
    if (!tracker->impl) {
        delete tracker;
        return nullptr;
    }

    tracker->sample_rate = sample_rate;
    tracker->df_increment = df_increment;
    return tracker;
}

void qm_tempo_free(QmTempoTracker* tracker) {
    if (tracker) {
        delete tracker->impl;
        delete tracker;
    }
}

int qm_tempo_calculate_beat_period(
    QmTempoTracker* tracker,
    const double* df, int df_len,
    double* beat_periods, double* tempi, int* out_len
) {
    if (!tracker || !tracker->impl || !df || !beat_periods || !tempi || !out_len) {
        return -1;
    }

    if (df_len <= 0) {
        *out_len = 0;
        return 0;
    }

    // Convert input to std::vector
    std::vector<double> df_vec(df, df + df_len);

    // IMPORTANT: beat_period must be pre-sized to df_len as the C++ code
    // writes directly into it by index (see TempoTrackV2.cpp line 340)
    std::vector<double> bp_vec(df_len, 0.0);

    // tempi is appended to, so should be empty
    std::vector<double> tempi_vec;

    // Call TempoTrackV2
    tracker->impl->calculateBeatPeriod(df_vec, bp_vec, tempi_vec);

    // Copy results to output arrays
    int len = static_cast<int>(tempi_vec.size());
    if (len > df_len) {
        len = df_len;  // Safety: don't exceed provided buffer
    }

    for (int i = 0; i < len; i++) {
        beat_periods[i] = bp_vec[i];
        tempi[i] = tempi_vec[i];
    }

    *out_len = len;
    return 0;
}

int qm_tempo_calculate_beats(
    QmTempoTracker* tracker,
    const double* df, int df_len,
    const double* beat_periods, int bp_len,
    double* beats, int* beats_len
) {
    if (!tracker || !tracker->impl || !df || !beat_periods || !beats || !beats_len) {
        return -1;
    }

    if (df_len <= 0 || bp_len <= 0) {
        *beats_len = 0;
        return 0;
    }

    // Convert inputs to std::vector
    std::vector<double> df_vec(df, df + df_len);
    std::vector<double> bp_vec(beat_periods, beat_periods + bp_len);
    std::vector<double> beats_vec;

    // Call TempoTrackV2
    tracker->impl->calculateBeats(df_vec, bp_vec, beats_vec);

    // Copy results to output array
    int len = static_cast<int>(beats_vec.size());
    if (len > df_len) {
        len = df_len;  // Safety: don't exceed provided buffer
    }

    for (int i = 0; i < len; i++) {
        beats[i] = beats_vec[i];
    }

    *beats_len = len;
    return 0;
}

} // extern "C"
