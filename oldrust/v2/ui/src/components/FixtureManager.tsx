import React, { useState } from 'react';

interface Fixture {
  id: number;
  name: string;
  type: string;
}

const FixtureManager: React.FC = () => {
  const [fixtures, setFixtures] = useState<Fixture[]>([]);

  const addFixture = () => {
    const newFixture: Fixture = {
      id: Date.now(),
      name: `Fixture ${fixtures.length + 1}`,
      type: 'Generic',
    };
    setFixtures([...fixtures, newFixture]);
  };

  return (
    <div className="fixture-manager">
      <h2>Fixture Manager</h2>
      <button onClick={addFixture}>Add Fixture</button>
      <ul>
        {fixtures.map((fixture) => (
          <li key={fixture.id}>{fixture.name} - {fixture.type}</li>
        ))}
      </ul>
    </div>
  );
};

export default FixtureManager;
