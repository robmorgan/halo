package rhythm

import (
	"math"
	"sync"
	"time"
)

// Metronome struct represents the Metronome in Go
// Originally based on https://github.com/Deep-Symmetry/electro/blob/main/src/main/java/org/deepsymmetry/electro/Metronome.java#L449
type Metronome struct {
	mu            sync.Mutex
	startTime     time.Time
	tempo         float64
	beatsPerBar   int
	barsPerPhrase int
}

func (m *Metronome) GetSnapshot(addedDuration time.Duration) *MetronomeSnapshot {
	// Implement the logic to create a snapshot
	//return Snapshot{Instant: time.Now().Add(addedDuration)}
	return NewMetronomeSnapshotWithInstant(time.Now().Add(addedDuration))
}

// NewMetronome creates a new Metronome with default values
func NewMetronome() *Metronome {
	return &Metronome{
		startTime:     time.Now(),
		tempo:         120.0,
		beatsPerBar:   4,
		barsPerPhrase: 8,
	}
}

// CopyMetronome creates a new Metronome as a copy of another
func CopyMetronome(m *Metronome) *Metronome {
	return &Metronome{
		startTime:     m.startTime,
		tempo:         m.tempo,
		beatsPerBar:   m.beatsPerBar,
		barsPerPhrase: m.barsPerPhrase,
	}
}

func (m *Metronome) GetTempo() float64 {
	return m.tempo
}

// SetTempo sets a new tempo for the Metronome. The start time will be adjusted so that the current beat and phase are
// unaffected by the tempo change.
func (m *Metronome) SetTempo(bpm float64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	// final long instant = System.currentTimeMillis();
	//     final long start = startTime.get();
	//     final double interval = getBeatInterval();
	//     final long beat = markerNumber(instant, start, interval);
	//     final double phase = markerPhase(instant, start, interval);
	//     final double newInterval = beatsToMilliseconds(1, bpm);
	//     startTime.set(instant - Math.round((newInterval * (phase + beat - 1))));
	//     tempo.set(bpm);

	instant := time.Now()
	interval := m.GetBeatInterval()
	beat := markerNumber(instant, m.startTime, interval)
	phase := markerPhase(instant, m.startTime, interval)
	newInterval := beatsToMilliseconds(1, bpm)
	m.startTime = instant.Add(-time.Duration(math.Round(newInterval * (phase + float64(beat) - 1))))
	m.tempo = bpm
}

// GetBeatInterval returns the number of milliseconds a beat lasts.
func (m *Metronome) GetBeatInterval() float64 {
	return beatsToMilliseconds(1, m.tempo)
}

// Other methods similar to Java implementation...

// beatsToMilliseconds calculates milliseconds for given beats and tempo
func beatsToMilliseconds(beats int, tempo float64) float64 {
	return (60000.0 / tempo) * float64(beats)
}

// markerNumber calculates the marker number
func markerNumber(instant, start time.Time, interval float64) int {
	return int(math.Floor(instant.Sub(start).Seconds()*1000/interval)) + 1
}

// markerPhase calculates the phase of a marker
func markerPhase(instant, start time.Time, interval float64) float64 {
	ratio := instant.Sub(start).Seconds() * 1000 / interval
	return ratio - math.Floor(ratio)
}
