import React, { useState } from 'react';

interface PatchAssignment {
  fixtureId: number;
  dmxAddress: number;
}

const PatchPanel: React.FC = () => {
  const [patchAssignments, setPatchAssignments] = useState<PatchAssignment[]>([]);

  const addPatchAssignment = () => {
    const newAssignment: PatchAssignment = {
      fixtureId: Math.floor(Math.random() * 1000),
      dmxAddress: Math.floor(Math.random() * 512) + 1,
    };
    setPatchAssignments([...patchAssignments, newAssignment]);
  };

  return (
    <div className="patch-panel">
      <h2>Patch Panel</h2>
      <button onClick={addPatchAssignment}>Add Patch Assignment</button>
      <ul>
        {patchAssignments.map((assignment, index) => (
          <li key={index}>Fixture ID: {assignment.fixtureId} - DMX Address: {assignment.dmxAddress}</li>
        ))}
      </ul>
    </div>
  );
};

export default PatchPanel;
