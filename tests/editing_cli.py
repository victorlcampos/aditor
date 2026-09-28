#!/usr/bin/env python3
"""Exercise editing against synthetic media; requires FFmpeg/ffprobe on PATH."""
import argparse
import json
import pathlib
import subprocess
import array
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--aditor', default='target/debug/aditor')
parser.add_argument('--font-file', help='Font for text rendering tests')
options = parser.parse_args()
binary = str(pathlib.Path(options.aditor).resolve())


def run(*args):
    result = subprocess.run(args, capture_output=True, timeout=60)
    assert result.returncode == 0, result.stderr.decode(errors='replace')
    return result.stdout


def probe(path):
    return json.loads(run('ffprobe', '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(path)))


def pixels(path, frame):
    return run('ffmpeg', '-v', 'error', '-i', str(path), '-vf', f'select=eq(n\\,{frame})',
               '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-')


with tempfile.TemporaryDirectory(prefix='aditor-edit-test-') as directory:
    root = pathlib.Path(directory)
    base, clip, still = [root / name for name in ('base.mp4', 'clip.mp4', 'still.png')]
    run('ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=red:s=160x120:r=10:d=2',
        '-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-c:v', 'libx264', '-c:a', 'aac', str(base))
    run('ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=blue:s=80x80:r=15:d=1',
        '-c:v', 'libx264', str(clip))
    run('ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=green:s=80x60',
        '-frames:v', '1', str(still))

    def edit(command, *args, name=None):
        output = root / (name or f'{command}.mp4')
        if command == 'write' and options.font_file:
            args = (*args, '--font-file', options.font_file)
        result = json.loads(run(binary, command, str(base), *map(str, args),
                                '--codec', 'h264-sw', '-o', str(output), '--json'))
        assert result['output'] == str(output)
        return output

    cropped = edit('crop', '--width', 80, '--height', 60, '--x', 10, '--y', 10,
                   '--from', 0.5, '--duration', 1)
    meta = probe(cropped)
    assert (meta['streams'][0]['width'], meta['streams'][0]['height']) == (80, 60)
    assert abs(float(meta['format']['duration']) - 1) < 0.15
    for source in (clip, still):
        for at in (0, 1, 2):
            output = edit('append', source, '--at', at, '--duration', 1, name=f'{source.stem}-{at}.mp4')
            meta = probe(output)
            assert abs(float(meta['format']['duration']) - 3) < 0.15, meta
            assert any(s['codec_type'] == 'audio' for s in meta['streams'])
            # Sample the center pixel: inserted footage must occupy its new slot.
            data = pixels(output, at * 10 + 5)
            rgb = data[(60 * 160 + 80)*3:][:3]
            assert rgb[2] > rgb[0] if source == clip else rgb[1] > rgb[0], rgb
            if at < 2:
                data = pixels(output, (at+1)*10+5)
                rgb = data[(60*160+80)*3:][:3]
                assert rgb[0] > 200 and rgb[2] < 30, rgb
    # Preserve inserted audio even when the original video is silent.
    silent = root / 'silent.mp4'
    run('ffmpeg', '-v', 'error', '-i', str(base), '-an', '-c:v', 'copy', str(silent))
    for source, has_audio in [(clip, False), (base, True)]:
        output = root / f'audio-{has_audio}.mp4'
        run(binary, 'append', str(silent), str(source), '--at', '1', '--duration', '1',
            '--codec', 'h264-sw', '-o', str(output))
        assert any(s['codec_type'] == 'audio' for s in probe(output)['streams']) == has_audio
        if has_audio:
            levels = []
            for start in (0.2, 1.2, 2.2):
                samples = array.array('h', run('ffmpeg', '-v', 'error', '-ss', str(start),
                    '-i', str(output), '-t', '0.2', '-f', 's16le', '-ac', '1', '-'))
                levels.append(sum(abs(s) for s in samples) / len(samples))
            assert levels[0] < 5 and levels[1] > 100 and levels[2] < 5, levels
    # Literal syntax must not create another filter or expand %{...}.
    written = edit('write', '--text', "It's: 100% %{n}, [ok]; \\ text", '--frame', 5, '--font-size', 12)
    assert pixels(written, 4) == pixels(written, 6)
    assert pixels(written, 4) != pixels(written, 5)
    interval = edit('write', '--text', 'Visible', '--from', 0.5, '--to', 1, name='interval.mp4')
    assert pixels(interval, 4) == pixels(interval, 10)
    assert pixels(interval, 4) != pixels(interval, 5)
    # Combine: side-by-side, stacked, and picture-in-picture of base + clip.
    def combine(*args, name):
        output = root / name
        result = json.loads(run(binary, 'combine', str(base), str(clip), *map(str, args),
                                '--codec', 'h264-sw', '-o', str(output), '--json'))
        assert result['output'] == str(output)
        return output

    side = combine('--layout', 'horizontal', name='side.mp4')
    meta = probe(side)
    assert (meta['streams'][0]['width'], meta['streams'][0]['height']) == (280, 120), meta
    assert abs(float(meta['format']['duration']) - 2) < 0.2, meta
    data = pixels(side, 5)
    left = data[(60 * 280 + 40) * 3:][:3]
    right = data[(60 * 280 + 220) * 3:][:3]
    assert left[0] > 200 and left[2] < 30, left
    assert right[2] > right[0], right
    stacked = combine('--layout', 'vertical', name='stacked.mp4')
    meta = probe(stacked)
    assert (meta['streams'][0]['width'], meta['streams'][0]['height']) == (160, 280), meta
    pip = combine('--layout', 'pip', name='pip.mp4')
    meta = probe(pip)
    assert (meta['streams'][0]['width'], meta['streams'][0]['height']) == (160, 120), meta
    short = combine('--duration', 'shortest', '--audio', 'mix', name='short.mp4')
    assert abs(float(probe(short)['format']['duration']) - 1) < 0.2
    assert any(s['codec_type'] == 'audio' for s in probe(short)['streams'])
    # Timer: stopwatch counts up, countdown counts down, window limits visibility.
    def timer(*args, name):
        output = root / name
        extra = list(args)
        if options.font_file:
            extra = [*extra, '--font-file', options.font_file]
        result = json.loads(run(binary, 'timer', str(base), *map(str, extra),
                                '--codec', 'h264-sw', '-o', str(output), '--json'))
        assert result['output'] == str(output)
        return output

    watch = timer(name='watch.mp4')
    assert pixels(watch, 5) != pixels(base, 5)
    assert pixels(watch, 5) != pixels(watch, 15)
    down = timer('--mode', 'countdown', name='down.mp4')
    assert pixels(down, 5) != pixels(watch, 5)
    windowed = timer('--from', 0.5, '--to', 1, name='windowed.mp4')
    assert pixels(windowed, 2) == pixels(windowed, 4)
    assert pixels(windowed, 2) != pixels(windowed, 7)
    # Overlay: burn the green still into the red base at a corner and window.
    def overlay(*args, name):
        output = root / name
        result = json.loads(run(binary, 'overlay', str(base), str(still), *map(str, args),
                                '--codec', 'h264-sw', '-o', str(output), '--json'))
        assert result['output'] == str(output)
        return output

    placed = overlay('--x', '0', '--y', '0', name='placed.mp4')
    corner = pixels(placed, 5)[(5 * 160 + 5) * 3:][:3]
    assert corner[1] > corner[0], corner
    assert abs(float(probe(placed)['format']['duration']) - 2) < 0.2
    assert any(s['codec_type'] == 'audio' for s in probe(placed)['streams'])
    timed_ov = overlay('--x', '0', '--y', '0', '--from', 0.5, '--to', 1, name='timed-ov.mp4')
    assert pixels(timed_ov, 2) == pixels(timed_ov, 4)
    assert pixels(timed_ov, 2) != pixels(timed_ov, 7)
    faded = overlay('--x', '0', '--y', '0', '--opacity', '0.5', '--width', '80',
                    name='faded.mp4')
    mid = pixels(faded, 5)[(5 * 160 + 5) * 3:][:3]
    assert mid[0] > 50 and mid[1] > 50, mid
    # A 1s video overlay shown until 1.5s freezes on its last frame past EOF.
    output = root / 'frozen.mp4'
    result = json.loads(run(binary, 'overlay', str(base), str(clip), '--x', '0', '--y', '0',
                            '--from', '0.5', '--to', '1.5',
                            '--codec', 'h264-sw', '-o', str(output), '--json'))
    assert result['output'] == str(output)
    late = pixels(output, 14)[(5 * 160 + 5) * 3:][:3]
    assert late[2] > late[0], late
    dry = root / 'dry.mp4'
    result = json.loads(run(binary, 'append', str(base), str(still), '--at', '1', '--duration', '1',
                           '--dry-run', '--json', '-o', str(dry)))
    assert result['dry_run'] and not dry.exists()
    dry_combine = root / 'dry-combine.mp4'
    result = json.loads(run(binary, 'combine', str(base), str(clip), '--layout', 'pip',
                           '--dry-run', '--json', '-o', str(dry_combine)))
    assert result['dry_run'] and not dry_combine.exists()
    dry_timer = root / 'dry-timer.mp4'
    timer_args = [binary, 'timer', str(base), '--mode', 'countdown',
                  '--dry-run', '--json', '-o', str(dry_timer)]
    if options.font_file:
        timer_args += ['--font-file', options.font_file]
    result = json.loads(run(*timer_args))
    assert result['dry_run'] and not dry_timer.exists()
    dry_overlay = root / 'dry-overlay.mp4'
    result = json.loads(run(binary, 'overlay', str(base), str(still), '--x', '10', '--y', '10',
                           '--dry-run', '--json', '-o', str(dry_overlay)))
    assert result['dry_run'] and not dry_overlay.exists()
    # Stroke: transparent pen animations that grow, then hold.
    def stroke(*args, name):
        output = root / name
        result = json.loads(run(binary, 'stroke', *map(str, args), '-o', str(output), '--json'))
        assert result['output'] == str(output)
        return output

    def alpha_pixels(path, frame):
        data = run('ffmpeg', '-v', 'error', '-ss', str(frame / 30),
                   '-i', str(path), '-frames:v', '1',
                   '-f', 'rawvideo', '-pix_fmt', 'argb', '-')
        return sum(1 for i in range(0, len(data), 4) if data[i] > 10)

    ring = stroke('--shape', 'ring', '--color', '#ff0000', '--width', '160',
                  '--height', '160', '--line-width', '8',
                  '--draw-duration', '0.5', '--hold-duration', '0.5', name='ring.mov')
    meta = probe(ring)
    assert (meta['streams'][0]['width'], meta['streams'][0]['height']) == (160, 160), meta
    assert meta['streams'][0]['pix_fmt'] == 'argb', meta
    assert abs(float(meta['format']['duration']) - 1) < 0.2, meta
    assert alpha_pixels(ring, 3) < alpha_pixels(ring, 9) <= alpha_pixels(ring, 24)
    for shape in ('underline', 'arrow', 'box'):
        other = stroke('--shape', shape, '--color', 'yellow', '--width', '160',
                       '--height', '120', '--draw-duration', '0.4', '--hold-duration', '0.4',
                       name=f'{shape}.mov')
        assert abs(float(probe(other)['format']['duration']) - 0.8) < 0.2
    dry_stroke = root / 'dry-stroke.mov'
    result = json.loads(run(binary, 'stroke', '--shape', 'box',
                           '--dry-run', '--json', '-o', str(dry_stroke)))
    assert result['dry_run'] and not dry_stroke.exists()
    # Frames: evenly spaced stills from the whole video or a slice.
    def frames(*args, name='shots'):
        output = root / name
        result = json.loads(run(binary, 'frames', str(base), *map(str, args),
                                '--dir', str(output), '--json'))
        assert result['dir'] == str(output.resolve())
        return output, result

    out, summary = frames('--count', 4)
    assert summary['count'] == 4 and len(summary['files']) == 4
    shot = probe(summary['files'][0])
    assert (shot['streams'][0]['width'], shot['streams'][0]['height']) == (160, 120)
    out, summary = frames('--fps', 5, '--from', 0.5, '--to', 1.5, name='sliced')
    assert summary['count'] == 5 and summary['from'] == 0.5
    out, summary = frames('--count', 2, '--format', 'jpg', name='thumbs')
    assert all(p.endswith('.jpg') for p in summary['files'])
    dry_shots = root / 'dry-shots'
    result = json.loads(run(binary, 'frames', str(base), '--fps', '2',
                           '--dry-run', '--json', '--dir', str(dry_shots)))
    assert result['dry_run'] and not dry_shots.exists()
    # Narrate: SRT to synchronized speech (needs an OS TTS engine).
    import os as _os
    import shutil as _shutil
    narrate_engine = None
    for candidate in ('say', 'espeak-ng'):
        if _shutil.which(candidate):
            narrate_engine = candidate.replace('-ng', '')
            break
    if narrate_engine is None and _os.name == 'nt' and (
            _shutil.which('powershell') or _shutil.which('pwsh')):
        narrate_engine = 'sapi'
    subs = root / 'tour.srt'
    subs.write_text(
        '1\n00:00:00,500 --> 00:00:01,500\nHello there\n\n'
        '2\n00:00:01,800 --> 00:00:02,800\nSecond line here\n')
    if narrate_engine is not None:
        try:
            speech = root / 'speech.wav'
            result = json.loads(run(binary, 'narrate', str(subs), '--engine', narrate_engine,
                                    '-o', str(speech), '--json'))
            assert result['output'] == str(speech)
            assert abs(float(probe(speech)['format']['duration']) - 2.8) < 0.3
            mixed = root / 'narrated.mp4'
            result = json.loads(run(binary, 'narrate', str(subs), '--video', str(base),
                                    '--engine', narrate_engine,
                                    '-o', str(mixed), '--json'))
            assert any(s['codec_type'] == 'audio' for s in probe(mixed)['streams'])
            dry_voice = root / 'dry-voice.wav'
            result = json.loads(run(binary, 'narrate', str(subs),
                                   '--dry-run', '--json', '-o', str(dry_voice)))
            assert result['dry_run'] and not dry_voice.exists()
        except AssertionError:
            raise
        except Exception as error:
            print(f'WARNING: skipping narrate checks (engine failed: {error})')
    else:
        print('WARNING: no TTS engine found, skipping narrate checks')
    (root / 'bad.srt').write_text('1\n00:00:02,000 --> 00:00:01,000\nBackwards\n')
    # Pipeline: chained steps with $refs run in dependency order.
    chain = {
        'steps': [
            {'id': 'clip', 'args': ['cut', str(base), '--from', '0.5', '--duration', '1',
                                    '--codec', 'h264-sw', '-o', str(root / 'chain-cut.mp4')]},
            {'id': 'marked', 'args': ['overlay', '$clip', str(still), '--x', '0', '--y', '0',
                                      '--codec', 'h264-sw', '-o', str(root / 'chain-ov.mp4')]},
        ],
        'output': '$marked',
    }
    recipe = root / 'chain.json'
    recipe.write_text(json.dumps(chain))
    result = json.loads(run(binary, 'pipeline', str(recipe), '--json'))
    assert result['output'] == str(root / 'chain-ov.mp4')
    assert [s['id'] for s in result['steps']] == ['clip', 'marked']
    assert pixels(root / 'chain-ov.mp4', 5) != pixels(base, 5)
    (root / 'cycle.json').write_text(json.dumps({'steps': [
        {'id': 'a', 'args': ['cut', '$b', '--duration', '1', '-o', str(root / 'a.mp4')]},
        {'id': 'b', 'args': ['cut', '$a', '--duration', '1', '-o', str(root / 'b.mp4')]},
    ]}))
    (root / 'ghost.json').write_text(json.dumps({'steps': [
        {'id': 'a', 'args': ['cut', '$ghost', '--duration', '1', '-o', str(root / 'a.mp4')]},
    ]}))
    # Cyclic and dangling recipes fail BEFORE running anything.
    for bad in ('cycle.json', 'ghost.json'):
        result = subprocess.run([binary, 'pipeline', str(root / bad)], capture_output=True)
        assert result.returncode != 0, bad
    assert not (root / 'a.mp4').exists() and not (root / 'b.mp4').exists()
    for args in [
        ['crop', str(base), '--width', '999', '--height', '60'],
        ['crop', str(base), '--duration', '0'],
        ['append', str(base), str(still), '--at', '1'],
        ['append', str(base), str(clip), '--at', '3'],
        ['write', str(base), '--text', 'Hi', '--frame', '20'],
        ['write', str(base), '--text', 'Hi', '--codec', 'copy'],
        ['combine', str(base), str(clip), '--layout', 'diagonal'],
        ['combine', str(base), str(clip), '--pip-scale', '2'],
        ['combine', str(base), str(clip), '--layout', 'horizontal', '-o', str(base), '--yes'],
        ['timer', str(base), '--mode', 'alarm'],
        ['timer', str(base), '--from', '5'],
        ['timer', str(base), '--text', 'Hi'],
        ['timer', str(base), '--codec', 'copy'],
        ['overlay', str(base), str(still), '--opacity', '2'],
        ['overlay', str(base), str(root / 'missing.png')],
        ['overlay', str(base), str(still), '--to', '1', '--duration', '1'],
        ['overlay', str(base), str(still), '-o', str(base), '--yes'],
        ['stroke', '--shape', 'star'],
        ['stroke', '--color', 'blurple'],
        ['stroke', '--fps', '0'],
        ['stroke', '--width', '0'],
        ['stroke', '--draw-duration', '0', '--hold-duration', '0'],
        ['stroke', '--line-width', '100', '--width', '160', '--height', '160'],
        ['frames', str(base)],
        ['frames', str(base), '--count', '2', '--fps', '2'],
        ['frames', str(base), '--count', '0'],
        ['frames', str(base), '--fps', '0'],
        ['frames', str(base), '--count', '2', '--format', 'bmp'],
        ['frames', str(base), '--count', '2', '--from', '5'],
        ['frames', str(base), '--count', '4', '--dir', str(root / 'shots')],
        ['pipeline', str(root / 'missing.json')],
        ['narrate', str(root / 'missing.srt')],
        ['narrate', str(subs), '--engine', 'flite'],
        ['narrate', str(subs), '-o', str(root / 'x.mp3')],
        ['narrate', str(subs), '--video', str(base), '-o', str(root / 'x.avi')],
        ['narrate', str(subs), '--volume', '5'],
        ['narrate', str(subs), '--video', str(base), '-o', str(base), '--yes'],
        ['narrate', str(root / 'bad.srt')],
        ['crop', str(base), '--duration', '1', '-o', str(base), '--yes'],
        ['crop', str(base), '--duration', '1', '-o', str(cropped)],
    ]:
        result = subprocess.run([binary, *args], capture_output=True)
        assert result.returncode != 0, args
    print('Editing integration checks passed.')
