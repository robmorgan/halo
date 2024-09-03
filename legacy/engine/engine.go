package engine

import (
	"runtime"
	"time"
)

type HaloLoop struct {
	onUpdate func(float64)
	tickRate time.Duration
	quit     chan bool
}

// Create new game loop
func New(tickRate time.Duration, onUpdate func(float64)) *HaloLoop {
	return &HaloLoop{
		onUpdate: onUpdate,
		tickRate: tickRate,
		quit:     make(chan bool),
	}
}

func (gl *HaloLoop) startLoop() {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()

	tickInterval := time.Second / gl.tickRate
	timeStart := time.Now().UnixNano()

	ticker := time.NewTicker(tickInterval)

	for {
		select {
		case <-ticker.C:
			now := time.Now().UnixNano()
			// DT in seconds
			delta := float64(now-timeStart) / 1000000000
			timeStart = now
			gl.onUpdate(delta)

		case <-gl.quit:
			ticker.Stop()
		}
	}
}

func (gl *HaloLoop) GetTickRate() time.Duration {
	return gl.tickRate
}

// Set tickRate and restart game loop
func (gl *HaloLoop) SetTickRate(tickRate time.Duration) {
	gl.tickRate = tickRate
	gl.Restart()
}

// Set onUpdate func
func (gl *HaloLoop) SetOnUpdate(onUpdate func(float64)) {
	gl.onUpdate = onUpdate
}

// Start game loop
func (gl *HaloLoop) Start() {
	go gl.startLoop()
}

// Stop game loop
func (gl *HaloLoop) Stop() {
	gl.quit <- true
}

// Restart game loop
func (gl *HaloLoop) Restart() {
	gl.Stop()
	gl.Start()
}
