package main

import (
	"os"
	"time"

	"github.com/faiface/beep/speaker"
	"github.com/faiface/beep/wav"
	"github.com/robmorgan/halo/logger"
)

func playAudio(file string) {
	logger := logger.GetProjectLogger()

	f, err := os.Open(file)
	if err != nil {
		logger.Fatal(err)
	}

	streamer, format, err := wav.Decode(f)
	if err != nil {
		logger.Fatal(err)
	}
	defer streamer.Close()

	speaker.Init(format.SampleRate, format.SampleRate.N(time.Second/10))
	speaker.Play(streamer)
	select {}
}
