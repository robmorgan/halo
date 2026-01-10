/* C wrapper for QM-DSP TempoTrackV2 */

#ifndef QM_DSP_WRAPPER_H
#define QM_DSP_WRAPPER_H

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to TempoTrackV2 instance */
typedef struct QmTempoTracker QmTempoTracker;

/**
 * Create a new TempoTrackV2 instance.
 *
 * @param sample_rate Audio sample rate (e.g., 44100.0)
 * @param df_increment Detection function frame increment (e.g., 512)
 * @return Pointer to new tracker, or NULL on failure
 */
QmTempoTracker* qm_tempo_new(float sample_rate, int df_increment);

/**
 * Free a TempoTrackV2 instance.
 *
 * @param tracker Pointer to tracker (may be NULL)
 */
void qm_tempo_free(QmTempoTracker* tracker);

/**
 * Calculate beat periods and tempi from a detection function.
 *
 * @param tracker Pointer to tracker
 * @param df Detection function values
 * @param df_len Length of detection function array
 * @param beat_periods Output array for beat periods (must be at least df_len elements)
 * @param tempi Output array for tempi in BPM (must be at least df_len elements)
 * @param out_len Pointer to receive actual output length
 * @return 0 on success, -1 on failure
 */
int qm_tempo_calculate_beat_period(
    QmTempoTracker* tracker,
    const double* df, int df_len,
    double* beat_periods, double* tempi, int* out_len
);

/**
 * Calculate beat positions from detection function and beat periods.
 *
 * @param tracker Pointer to tracker
 * @param df Detection function values
 * @param df_len Length of detection function array
 * @param beat_periods Beat periods from qm_tempo_calculate_beat_period
 * @param bp_len Length of beat periods array
 * @param beats Output array for beat positions (must be at least df_len elements)
 * @param beats_len Pointer to receive actual number of beats
 * @return 0 on success, -1 on failure
 */
int qm_tempo_calculate_beats(
    QmTempoTracker* tracker,
    const double* df, int df_len,
    const double* beat_periods, int bp_len,
    double* beats, int* beats_len
);

#ifdef __cplusplus
}
#endif

#endif /* QM_DSP_WRAPPER_H */
