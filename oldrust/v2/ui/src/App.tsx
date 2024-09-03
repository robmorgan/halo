import React, { useState } from 'react';
import Visualizer from './components/Visualizer';
import FixtureManager from './components/FixtureManager';
import PatchPanel from './components/PatchPanel';
import PlaybackEngine from './components/PlaybackEngine';
import './App.css'; // We'll create this file for styling

type Tab = 'fixtures' | 'patch' | 'playback';

const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<Tab>('fixtures');

  return (
    <div className="app">
      <div className="visualizer-container">
        <Visualizer />
      </div>
      <div className="control-panel">
        <div className="tab-buttons">
          <button 
            className={activeTab === 'fixtures' ? 'active' : ''} 
            onClick={() => setActiveTab('fixtures')}
          >
            Fixtures
          </button>
          <button 
            className={activeTab === 'patch' ? 'active' : ''} 
            onClick={() => setActiveTab('patch')}
          >
            Patch
          </button>
          <button 
            className={activeTab === 'playback' ? 'active' : ''} 
            onClick={() => setActiveTab('playback')}
          >
            Playback
          </button>
        </div>
        <div className="tab-content">
          {activeTab === 'fixtures' && <FixtureManager />}
          {activeTab === 'patch' && <PatchPanel />}
          {activeTab === 'playback' && <PlaybackEngine />}
        </div>
      </div>
    </div>
  );
};

export default App;
