'use strict';

const examples = {
  install: {
    command: './aditor --install',
    description: 'Install the extracted executable into your user directory and configure PATH for new terminals.',
    prerequisite: 'macOS/Linux: run this inside the extracted folder. Windows PowerShell: use .\\aditor.exe --install. Then open a new terminal and run aditor --version.',
  },
  update: {
    command: 'aditor --update',
    description: 'Download the latest release, verify SHA-256 and its version, then replace the CLI you are running.',
    prerequisite: 'Install first so you update the copy on your PATH. Requires internet access and write permission. Already current? The command exits without downloading the archive.',
  },
  record: {
    command: 'aditor record --tab ABC123 \\\n  --duration 15 \\\n  -o demo.mp4 --json',
    description: 'Record 15 seconds of one browser tab and return the finished file as JSON.',
    prerequisite: 'Requires a CDP-enabled browser. Replace ABC123 with an ID from aditor tabs --json.',
  },
  screenshot: {
    command: "aditor screenshot --tab ABC123 \\\n  --selector '#player' \\\n  -o player.png --json",
    description: 'Save a PNG of exactly one CSS-selected element, even outside the viewport.',
    prerequisite: 'Requires CDP and one rendered element in the main document. Selectors do not cross iframe or shadow DOM boundaries.',
  },
  edit: {
    command: 'aditor edit demo.mp4 \\\n  --from 2 --duration 10 \\\n  --speed 2 --mute \\\n  -o demo-final.mp4 --json',
    description: 'Take a 10-second segment, play it at 2× speed, and remove the audio in a single command.',
    prerequisite: 'Use an existing input video. FFmpeg is resolved automatically. The output file must not already exist unless you pass --yes.',
  },
  background: {
    command: 'aditor record --tab ABC123 \\\n  -o demo.mp4 --json\n\n# Later, use the returned recording ID:\naditor stop RECORDING_ID --json',
    description: 'Start recording in the background. Stop by session ID to finalize the MP4.',
    prerequisite: 'Requires a CDP-enabled browser. Replace ABC123 with your tab ID and RECORDING_ID with the ID returned by record.',
  },
};

const tabs = [...document.querySelectorAll('[data-task]')];
const commandCode = document.querySelector('#command-code');
const copyButton = document.querySelector('#copy-command');
const copyStatus = document.querySelector('#copy-status');
let currentTask = 'record';
let feedbackTimeout;

function selectTask(tab) {
  currentTask = tab.dataset.task;
  const example = examples[currentTask];
  for (const item of tabs) {
    item.setAttribute('aria-selected', String(item === tab));
    item.tabIndex = item === tab ? 0 : -1;
  }
  document.querySelector('#command-panel').setAttribute('aria-labelledby', tab.id);
  commandCode.textContent = example.command;
  document.querySelector('#command-description').textContent = example.description;
  document.querySelector('#command-prerequisite').textContent = example.prerequisite;
  clearTimeout(feedbackTimeout);
  copyButton.textContent = 'Copy command ⧉';
  copyStatus.textContent = '';
}

for (const tab of tabs) {
  tab.addEventListener('click', () => selectTask(tab));
  tab.addEventListener('keydown', (event) => {
    let index = tabs.indexOf(tab);
    if (event.key === 'ArrowDown' || event.key === 'ArrowRight') index = (index + 1) % tabs.length;
    else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') index = (index + tabs.length - 1) % tabs.length;
    else if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = tabs.length - 1;
    else return;
    event.preventDefault();
    tabs[index].focus();
    selectTask(tabs[index]);
  });
}

copyButton.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(examples[currentTask].command);
    copyButton.textContent = 'Copied ✓';
    copyStatus.textContent = 'Command copied to clipboard.';
  } catch {
    const range = document.createRange();
    range.selectNodeContents(commandCode);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    copyButton.textContent = 'Select & copy';
    copyStatus.textContent = 'Automatic copying is unavailable. The command is selected; use your keyboard to copy it.';
  }
  clearTimeout(feedbackTimeout);
  feedbackTimeout = setTimeout(() => { copyButton.textContent = 'Copy command ⧉'; }, 2500);
});

// The download links work independently of this optional release label.
fetch('https://api.github.com/repos/victorlcampos/aditor/releases/latest', { signal: AbortSignal.timeout(5000) })
  .then((response) => {
    if (!response.ok) throw new Error('Release metadata unavailable');
    return response.json();
  })
  .then((release) => {
    if (typeof release.tag_name === 'string') {
      document.querySelector('#release-label').textContent = release.tag_name;
    }
  })
  .catch(() => { /* Keep the permanent latest-release link and fallback label. */ });
