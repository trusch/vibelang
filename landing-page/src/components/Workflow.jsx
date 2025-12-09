import React from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './Workflow.css';

const steps = [
  {
    number: '01',
    title: 'Write',
    description: 'Create a .vibe file in your favorite editor. Import sounds, define patterns, write melodies.',
    code: 'let kick = voice("kick").synth("kick_808");'
  },
  {
    number: '02',
    title: 'Save',
    description: 'Hit Cmd+S. Watch mode detects the change and recompiles in milliseconds.',
    code: '// Changes detected, recompiling...'
  },
  {
    number: '03',
    title: 'Hear',
    description: 'Your music updates in real-time. No restart, no waiting. Iterate at the speed of thought.',
    code: '// Now playing: beat.vibe [120 BPM]'
  }
];

function Workflow() {
  return (
    <section className="workflow">
      <div className="container">
        <div className="workflow__header">
          <span className="workflow__label">// workflow</span>
          <h2>Edit. Save. Hear.</h2>
          <p className="workflow__subtitle">
            The fastest feedback loop in music production. No DAW load times. No bouncing. Just code and sound.
          </p>
        </div>

        <div className="workflow__steps">
          {steps.map((step, index) => (
            <div key={index} className="workflow-step">
              <div className="workflow-step__number">{step.number}</div>
              <div className="workflow-step__content">
                <h3 className="workflow-step__title">{step.title}</h3>
                <p className="workflow-step__description">{step.description}</p>
                <div className="workflow-step__code">
                  <code>{highlightCode(step.code)}</code>
                </div>
              </div>
              {index < steps.length - 1 && (
                <div className="workflow-step__connector">
                  <span>{'>'}</span>
                </div>
              )}
            </div>
          ))}
        </div>

        <div className="workflow__features">
          <div className="workflow-feature">
            <span className="workflow-feature__icon">{'<>'}</span>
            <h4>Version Control</h4>
            <p>Music as text means git diff, git blame, git branch. Collaborate like you code.</p>
          </div>
          <div className="workflow-feature">
            <span className="workflow-feature__icon">{'{}'}</span>
            <h4>Editor Agnostic</h4>
            <p>VS Code, Vim, Emacs, Sublime—use whatever you love. It's just text.</p>
          </div>
          <div className="workflow-feature">
            <span className="workflow-feature__icon">{'()'}</span>
            <h4>Reproducible</h4>
            <p>Your .vibe file is the single source of truth. Share it, and they hear exactly what you hear.</p>
          </div>
        </div>
      </div>
    </section>
  );
}

export default Workflow;
