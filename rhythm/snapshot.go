package rhythm

// Snapshot is an interface for probing details about the timeline established by a metronome.
type Snapshot interface {
	// GetStartTime gets the metronome's timeline origin.
	GetStartTime() int64

	// GetTempo gets the metronome's tempo.
	GetTempo() float64

	// GetBeatsPerBar gets the metronome's bar length in beats.
	GetBeatsPerBar() int

	// GetBarsPerPhrase gets the metronome's phrase length in bars.
	GetBarsPerPhrase() int

	// GetInstant gets the point in time with respect to which the snapshot is computed.
	GetInstant() int64

	// GetBeatInterval gets the metronome's beat length in time.
	GetBeatInterval() float64

	// GetBarInterval gets the metronome's bar length in time.
	GetBarInterval() float64

	// GetPhraseInterval gets the metronome's phrase length in time.
	GetPhraseInterval() float64

	// GetBeat gets the metronome's beat number.
	GetBeat() int64

	// GetBar gets the metronome's bar number.
	GetBar() int64

	// GetPhrase gets the metronome's phrase number.
	GetPhrase() int64

	// GetBeatPhase gets the metronome's beat phase at the time of the snapshot.
	GetBeatPhase() float64

	// GetBarPhase gets the metronome's bar phase at the time of the snapshot.
	GetBarPhase() float64

	// GetPhrasePhase gets the metronome's phrase phase at the time of the snapshot.
	GetPhrasePhase() float64

	// GetTimeOfBeat determines the timestamp at which a particular beat will occur.
	GetTimeOfBeat(beat int64) int64

	// GetBeatWithinBar returns the beat number of the snapshot relative to the start of the bar.
	GetBeatWithinBar() int

	// IsDownBeat checks whether the current beat at the time of the snapshot was the first beat in its bar.
	IsDownBeat() bool

	// GetBeatWithinPhrase returns the beat number of the snapshot relative to the start of the phrase.
	GetBeatWithinPhrase() int

	// IsPhraseStart checks whether the current beat at the time of the snapshot was the first beat in its phrase.
	IsPhraseStart() bool

	// GetTimeOfBar determines the timestamp at which a particular bar will occur.
	GetTimeOfBar(bar int64) int64

	// GetBarWithinPhrase returns the bar number of the snapshot relative to the start of the phrase.
	GetBarWithinPhrase() int

	// GetTimeOfPhrase determines the timestamp at which a particular phrase will occur.
	GetTimeOfPhrase(phrase int64) int64

	// GetMarker returns the time represented by the snapshot as "phrase.bar.beat".
	GetMarker() string

	// DistanceFromBeat determines how far in time the snapshot is from its closest beat.
	DistanceFromBeat() float64

	// DistanceFromBar determines how far in time the snapshot is from its closest bar boundary.
	DistanceFromBar() float64

	// DistanceFromPhrase determines how far in time the snapshot is from its closest phrase boundary.
	DistanceFromPhrase() float64
}
