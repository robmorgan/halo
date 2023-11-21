package main

import (
	"fmt"
	"math"
	"math/rand"
	"time"
)

func main() {
	rand.Seed(time.Now().UTC().UnixNano())

	//startTime := time.Now()

	go startBeat(60) // Start beat for 120 BPM

	for { // sleep indefinitely
	}
	// for {
	// 	currentTime := time.Now()
	// 	value := applySawtoothEffect(startTime, currentTime)

	// 	// TODO Send the DMX packet with updated values
	// 	// For now we simply output the value to the console
	// 	fmt.Printf("current value: %f\n", value)
	// }
}

func startBeat(bpm int) {
	startTime := time.Now()
	beatInterval := time.Minute / time.Duration(bpm)
	ticker := time.NewTicker(beatInterval)
	for range ticker.C {
		// TODO Trigger lighting event

		currentTime := time.Now()
		value := applySawtoothEffect(startTime, currentTime)

		// TODO Send the DMX packet with updated values
		// For now we simply output the value to the console
		fmt.Printf("beat interval=%d, current value: %f\n", beatInterval, value)
	}

}

// The sawtooth curve can be used to modulate the intensity or other parameters of the light.
// Calculate the value of the sawtooth wave at each beat.
func sawtooth(t float64) float64 {
	return 2 * (t/math.Pi - math.Floor(0.5+t/math.Pi))
}

func applySawtoothEffect(startTime, t time.Time) float64 {
	// Calculate the sawtooth value at time t
	value := sawtooth(t.Sub(startTime).Seconds())
	return value
}
