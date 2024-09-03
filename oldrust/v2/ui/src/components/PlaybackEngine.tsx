import React, { useState } from 'react';

interface Cue {
  id: number;
  name: string;
}

const PlaybackEngine: React.FC = () => {
  const [cues, setCues] = useState<Cue[]>([]);
  const [currentCue, setCurrentCue] = useState<Cue | null>(null);

  const addCue = () => {
    const newCue: Cue = {
      id: Date.now(),
      name: `Cue ${cues.length + 1}`,
    };
    setCues([...cues, newCue]);
  };

  const playCue = (cue: Cue) => {
    setCurrentCue(cue);
    // Add logic to actually play the cue
  };

  return (
    <div className="playback-engine">
      <h2>Playback Engine</h2>
      <button onClick={addCue}>Add Cue</button>
      <ul>
        {cues.map((cue) => (
          <li key={cue.id}>
            {cue.name}
            <button onClick={() => playCue(cue)}>Play</button>
          </li>
        ))}
      </ul>
      {currentCue && <p>Current Cue: {currentCue.name}</p>}
    </div>
  );
};

export default PlaybackEngine;
