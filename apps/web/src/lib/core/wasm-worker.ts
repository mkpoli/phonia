import init, {
	WasmBitDepth,
	WasmColormap,
	WasmContentHasher,
	WasmEngine,
	WasmTierRelation,
	contentHash as wasmContentHash,
	exportFigure as wasmExportFigure,
	loadProjectContainer as wasmLoadProjectContainer,
	readProjectBundle as wasmReadProjectBundle,
	renameProjectContainer as wasmRenameProjectContainer,
	renderFigureSvg as wasmRenderFigureSvg,
	wavStreamHeader as wasmWavStreamHeader
} from '../wasm/pkg/phx_wasm.js';
import type {
  AnnotationId,
  AudioId,
  BoundaryId,
  FigureExportFormat,
  FigureSpec,
  IntervalId,
  PointId,
  SpectrogramTileRequest,
  TierId,
  TierInfo,
  WavBitDepth
} from './types';

type RequestMessage =
  | { id: number; method: 'importAudio'; bytes: ArrayBuffer; name: string }
  | {
      id: number;
      method: 'openAudioFile';
      dirSegments: string[];
      fileName: string;
      name: string;
    }
  | { id: number; method: 'beginRecording'; sampleRate: number; channels: number }
  | { id: number; method: 'appendSamples'; recordingId: bigint; samples: ArrayBuffer }
  | { id: number; method: 'finishRecording'; recordingId: bigint; name: string }
  | { id: number; method: 'abortRecording'; recordingId: bigint }
  | { id: number; method: 'waveformSlice'; audioId: AudioId; t0: number; t1: number; px: number }
  | { id: number; method: 'samplesInRange'; audioId: AudioId; t0: number; t1: number }
  | { id: number; method: 'spectrogramTile'; audioId: AudioId; req: SpectrogramTileRequest }
  | { id: number; method: 'spectrogramProbe'; audioId: AudioId; req: SpectrogramTileRequest }
  | { id: number; method: 'pitchTrack'; audioId: AudioId; floorHz: number; ceilingHz: number }
  | { id: number; method: 'pulseTimes'; audioId: AudioId; floorHz: number; ceilingHz: number }
  | { id: number; method: 'spectrumSlice'; audioId: AudioId; t0: number; t1: number }
  | { id: number; method: 'lpcSpectrum'; audioId: AudioId; t0: number; t1: number }
  | { id: number; method: 'ltas'; audioId: AudioId; t0: number; t1: number }
  | {
      id: number;
      method: 'silenceIntervals';
      audioId: AudioId;
      thresholdDb: number;
      minSilentS: number;
      minSoundingS: number;
    }
  | {
      id: number;
      method: 'voicingIntervals';
      audioId: AudioId;
      pitchFloorHz: number;
      pitchCeilingHz: number;
      minVoicedS: number;
      minUnvoicedS: number;
    }
  | {
      id: number;
      method: 'pitchTrackSpan';
      audioId: AudioId;
      floorHz: number;
      ceilingHz: number;
      t0: number;
      t1: number;
    }
  | {
      id: number;
      method: 'formantTrack';
      audioId: AudioId;
      ceilingHz: number;
      maxFormants: number;
      smoothed: boolean;
    }
  | { id: number; method: 'intensityTrack'; audioId: AudioId; floorHz: number }
  | {
      id: number;
      method: 'bandEnergy';
      audioId: AudioId;
      t0: number;
      t1: number;
      f0: number;
      f1: number;
    }
  | { id: number; method: 'nearestZeroCrossing'; audioId: AudioId; t: number }
  | { id: number; method: 'harmonicityTrack'; audioId: AudioId; t0: number; t1: number }
  | { id: number; method: 'cppTrack'; audioId: AudioId; t0: number; t1: number }
  | {
      id: number;
      method: 'bandFilteredSpan';
      audioId: AudioId;
      t0: number;
      t1: number;
      fLow: number;
      fHigh: number;
    }
  | {
      id: number;
      method: 'selectionReadout';
      audioId: AudioId;
      t0: number;
      t1: number;
      f0: number;
      f1: number;
      pitchFloorHz: number;
      pitchCeilingHz: number;
      intensityFloorHz: number;
    }
  | {
      id: number;
      method: 'formantSpanMeans';
      audioId: AudioId;
      ceilingHz: number;
      maxFormants: number;
      smoothed: boolean;
      t0: number;
      t1: number;
    }
  | {
      id: number;
      method: 'voiceReport';
      audioId: AudioId;
      t0: number;
      t1: number;
      pitchFloorHz: number;
      pitchCeilingHz: number;
    }
  | { id: number; method: 'createAnnotation'; audioId: AudioId; xmin: number; xmax: number }
  | { id: number; method: 'addIntervalTier'; annotationId: AnnotationId; name: string }
  | { id: number; method: 'addPointTier'; annotationId: AnnotationId; name: string }
  | {
      id: number;
      method: 'renameTier';
      annotationId: AnnotationId;
      tierId: TierId;
      name: string;
    }
  | { id: number; method: 'removeTier'; annotationId: AnnotationId; tierId: TierId }
  | {
      id: number;
      method: 'reorderTier';
      annotationId: AnnotationId;
      tierId: TierId;
      toIndex: number;
    }
  | { id: number; method: 'duplicateTier'; annotationId: AnnotationId; tierId: TierId }
  | { id: number; method: 'insertBoundary'; annotationId: AnnotationId; tierId: TierId; at: number }
  | {
      id: number;
      method: 'insertPoint';
      annotationId: AnnotationId;
      tierId: TierId;
      time: number;
      label: string;
    }
  | {
      id: number;
      method: 'moveBoundary';
      annotationId: AnnotationId;
      boundaryId: BoundaryId;
      to: number;
      linked: boolean;
    }
  | { id: number; method: 'removeBoundary'; annotationId: AnnotationId; boundaryId: BoundaryId }
  | { id: number; method: 'removePoint'; annotationId: AnnotationId; pointId: PointId }
  | {
      id: number;
      method: 'setIntervalLabel';
      annotationId: AnnotationId;
      tierId: TierId;
      intervalId: IntervalId;
      text: string;
    }
  | {
      id: number;
      method: 'setPointLabel';
      annotationId: AnnotationId;
      tierId: TierId;
      pointId: PointId;
      text: string;
    }
  | { id: number; method: 'renameAudio'; audioId: AudioId; name: string }
  | { id: number; method: 'detachAudio'; audioId: AudioId }
  | { id: number; method: 'undo' }
  | { id: number; method: 'redo' }
  | { id: number; method: 'undoDepth' }
  | { id: number; method: 'redoDepth' }
  | { id: number; method: 'journalHeadId' }
  | { id: number; method: 'stateHash' }
  | { id: number; method: 'listAnnotations'; audioId: AudioId }
  | { id: number; method: 'annotationTiers'; annotationId: AnnotationId }
  | {
      id: number;
      method: 'intervalsInRange';
      annotationId: AnnotationId;
      tierId: TierId;
      t0: number;
      t1: number;
    }
  | {
      id: number;
      method: 'pointsInRange';
      annotationId: AnnotationId;
      tierId: TierId;
      t0: number;
      t1: number;
    }
  | { id: number; method: 'searchLabels'; pattern: string; regex: boolean }
  | { id: number; method: 'importTextGrid'; audioId: AudioId; bytes: ArrayBuffer }
  | { id: number; method: 'exportTextGrid'; annotationId: AnnotationId }
  | { id: number; method: 'annotationJson'; annotationId: AnnotationId }
  | { id: number; method: 'attachAnnotationJson'; audioId: AudioId; json: string }
  | { id: number; method: 'saveProjectContainer'; specJson: string }
  | { id: number; method: 'saveProjectBundle'; specJson: string; ids: number[]; media: ArrayBuffer[] }
  | { id: number; method: 'loadProjectContainer'; bytes: ArrayBuffer }
  | { id: number; method: 'readProjectBundle'; bytes: ArrayBuffer }
  | { id: number; method: 'renameProjectContainer'; bytes: ArrayBuffer; name: string }
  | { id: number; method: 'contentHash'; bytes: ArrayBuffer }
  | { id: number; method: 'exportSpanWav'; audioId: AudioId; t0: number; t1: number; bits: WavBitDepth }
  | { id: number; method: 'reverseSpanWav'; audioId: AudioId; t0: number; t1: number; bits: WavBitDepth }
  | {
      id: number;
      method: 'scaleIntensitySpanWav';
      audioId: AudioId;
      t0: number;
      t1: number;
      targetDb: number;
      bits: WavBitDepth;
    }
  | {
      id: number;
      method: 'scalePeakSpanWav';
      audioId: AudioId;
      t0: number;
      t1: number;
      target: number;
      bits: WavBitDepth;
    }
  | { id: number; method: 'resampleWav'; audioId: AudioId; targetHz: number; bits: WavBitDepth }
  | { id: number; method: 'convertToMono'; audioId: AudioId; bits: WavBitDepth }
  | { id: number; method: 'concatWav'; ids: number[]; bits: WavBitDepth }
  | {
      id: number;
      method: 'exportBandFilteredSpanWav';
      audioId: AudioId;
      t0: number;
      t1: number;
      fLow: number;
      fHigh: number;
      bits: WavBitDepth;
    }
  | {
      id: number;
      method: 'exportNotchFilteredSpanWav';
      audioId: AudioId;
      t0: number;
      t1: number;
      fLow: number;
      fHigh: number;
      bits: WavBitDepth;
    }
  | { id: number; method: 'exportChannelWav'; audioId: AudioId; channel: number; bits: WavBitDepth }
  | {
      id: number;
      method: 'applyPreemphasisWav';
      audioId: AudioId;
      t0: number;
      t1: number;
      fromHz: number;
      bits: WavBitDepth;
    }
  | { id: number; method: 'subtractMeanWav'; audioId: AudioId; t0: number; t1: number; bits: WavBitDepth }
  | { id: number; method: 'buildFigure'; spec: FigureSpec }
  | { id: number; method: 'renderFigureSvg'; figureJson: string }
  | { id: number; method: 'exportFigure'; figureJson: string; format: FigureExportFormat };

type ResponseMessage =
  | { id: number; ok: true; result: unknown }
  | { id: number; ok: false; error: string };

let enginePromise: Promise<WasmEngine> | null = null;

function engine() {
  enginePromise ??= init().then(() => new WasmEngine());
  return enginePromise;
}

/** Bytes read per chunk when hashing or whole-reading an OPFS file. */
const OPFS_READ_CHUNK = 1 << 20;

/**
 * The subset of `FileSystemSyncAccessHandle` this worker uses. The type is only
 * exposed inside a worker, and not yet in every `lib.dom` release, so it is
 * declared locally.
 */
interface SyncAccessHandle {
  read(buffer: ArrayBufferView, options?: { at?: number }): number;
  getSize(): number;
  close(): void;
}

interface SyncCapableFileHandle {
  createSyncAccessHandle(options?: {
    mode?: 'read-only' | 'readwrite' | 'readwrite-unsafe';
  }): Promise<SyncAccessHandle>;
}

/**
 * Synchronous access handles kept open for the life of each streamed source.
 *
 * A streamed entry's `readAt` callback reads through its handle on every
 * waveform, spectrogram, and analysis query, so the handle must outlive the
 * import call. It is opened read-only, which lets the main thread still read the
 * same file (playback, export) without contending for an exclusive lock.
 */
const streamedHandles = new Map<bigint, SyncAccessHandle>();

/** Walks OPFS `dirSegments` and opens a read-only sync handle on `fileName`. */
async function openSyncHandle(
  dirSegments: string[],
  fileName: string
): Promise<SyncAccessHandle> {
  let dir = await navigator.storage.getDirectory();
  for (const segment of dirSegments) {
    dir = await dir.getDirectoryHandle(segment);
  }
  const fileHandle = (await dir.getFileHandle(fileName)) as unknown as SyncCapableFileHandle;
  try {
    return await fileHandle.createSyncAccessHandle({ mode: 'read-only' });
  } catch {
    // Older engines reject the options bag; fall back to the default mode.
    return await fileHandle.createSyncAccessHandle();
  }
}

/** A `readAt(offset, length)` callback that serves ranges from a sync handle. */
function readAtOf(handle: SyncAccessHandle) {
  return (offset: number, length: number): Uint8Array => {
    const buffer = new Uint8Array(length);
    const read = handle.read(buffer, { at: offset });
    if (read !== length) {
      throw new Error(`OPFS read returned ${read} of ${length} bytes at ${offset}`);
    }
    return buffer;
  };
}

/**
 * Detects a WAV, AIFF, or FLAC container from its leading bytes, mirroring
 * `phx_audio::sniff_container`. Only WAV supports the streamed-open path
 * ({@link wasmWavStreamHeader} parses a RIFF/WAVE header), so the worker reads
 * this before deciding whether a stored file is a streaming candidate or
 * belongs straight on the eager decode path.
 */
function sniffContainer(head: Uint8Array): 'wav' | 'aiff' | 'flac' | null {
  if (head.length < 12) return null;
  const ascii = (start: number, end: number) => String.fromCharCode(...head.subarray(start, end));
  if (ascii(0, 4) === 'fLaC') return 'flac';
  if (ascii(0, 4) === 'RIFF' && ascii(8, 12) === 'WAVE') return 'wav';
  if (ascii(0, 4) === 'FORM' && (ascii(8, 12) === 'AIFF' || ascii(8, 12) === 'AIFC')) return 'aiff';
  return null;
}

function colormap(name: SpectrogramTileRequest['colormap']): WasmColormap {
  return WasmColormap[name];
}

type WasmAppliedLike = {
  kind: string;
  annotation?: bigint;
  audio?: bigint;
  tier?: bigint;
  boundary?: bigint;
};

function appliedToPlain(applied: WasmAppliedLike | undefined | null) {
  if (!applied) return null;
  return {
    kind: applied.kind,
    annotation: applied.annotation,
    audio: applied.audio,
    tier: applied.tier,
    boundary: applied.boundary
  };
}

function tiersToPlain(list: {
  ids: BigUint64Array;
  names: string[];
  kinds: Uint8Array;
}): TierInfo[] {
  const out: TierInfo[] = [];
  for (let i = 0; i < list.ids.length; i += 1) {
    out.push({ id: list.ids[i], name: list.names[i], kind: list.kinds[i] === 1 ? 'point' : 'interval' });
  }
  return out;
}

function intervalsToPlain(list: {
  ids: BigUint64Array;
  startBoundaries: BigUint64Array;
  endBoundaries: BigUint64Array;
  xmin: Float64Array;
  xmax: Float64Array;
  labels: string[];
}) {
  const out = [];
  for (let i = 0; i < list.ids.length; i += 1) {
    out.push({
      id: list.ids[i],
      startBoundary: list.startBoundaries[i],
      endBoundary: list.endBoundaries[i],
      xmin: list.xmin[i],
      xmax: list.xmax[i],
      label: list.labels[i]
    });
  }
  return out;
}

function pointsToPlain(list: { ids: BigUint64Array; times: Float64Array; labels: string[] }) {
  const out = [];
  for (let i = 0; i < list.ids.length; i += 1) {
    out.push({ id: list.ids[i], time: list.times[i], label: list.labels[i] });
  }
  return out;
}

function hitsToPlain(list: {
  annotations: BigUint64Array;
  tiers: BigUint64Array;
  kinds: Uint8Array;
  targets: BigUint64Array;
  starts: Uint32Array;
  ends: Uint32Array;
}) {
  const out = [];
  for (let i = 0; i < list.annotations.length; i += 1) {
    out.push({
      annotation: list.annotations[i],
      tier: list.tiers[i],
      kind: list.kinds[i] === 1 ? ('point' as const) : ('interval' as const),
      target: list.targets[i],
      start: list.starts[i],
      end: list.ends[i]
    });
  }
  return out;
}

self.onmessage = async (event: MessageEvent<RequestMessage>) => {
  const message = event.data;
  try {
    const wasm = await engine();
    switch (message.method) {
      case 'importAudio': {
        const bytes = new Uint8Array(message.bytes);
        const importedId = wasm.importAudioBytes(bytes);
        const hash = wasmContentHash(bytes);
        const info = wasm.audioInfo(importedId);
        const result = {
          id: importedId,
          duration: info.duration,
          sampleRate: info.sampleRate,
          channels: info.channels,
          name: message.name || info.name || undefined,
          hash
        };
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'openAudioFile': {
        // The file already lives in OPFS. A sync access handle serves its bytes
        // without them ever crossing postMessage: a short take is read whole and
        // decoded eagerly, a long one is opened streamed so only ranged reads
        // reach wasm and the decoded signal never resides in memory. Streaming
        // is a WAV-only path (the header parse below is RIFF-specific), so an
        // AIFF or FLAC file always takes the eager branch regardless of length.
        const handle = await openSyncHandle(message.dirSegments, message.fileName);
        const size = handle.getSize();
        const readAt = readAtOf(handle);
        let keepHandle = false;
        try {
          const container = size >= 12 ? sniffContainer(readAt(0, 12)) : null;
          const header = container === 'wav' ? wasmWavStreamHeader(size, readAt) : null;
          const streamed = header !== null && header.frames > WasmEngine.eagerImportFrameLimit();

          let audioId: bigint;
          let hash: string;
          if (streamed) {
            audioId = wasm.openStreamingWav(size, message.name, readAt);
            const hasher = new WasmContentHasher();
            const chunk = new Uint8Array(OPFS_READ_CHUNK);
            for (let offset = 0; offset < size; ) {
              const want = Math.min(OPFS_READ_CHUNK, size - offset);
              const read = handle.read(chunk.subarray(0, want), { at: offset });
              if (read <= 0) break;
              hasher.update(chunk.subarray(0, read));
              offset += read;
            }
            hash = hasher.finalizeHex();
            streamedHandles.set(audioId, handle);
            keepHandle = true;
          } else {
            const bytes = new Uint8Array(size);
            for (let offset = 0; offset < size; ) {
              const read = handle.read(bytes.subarray(offset), { at: offset });
              if (read <= 0) break;
              offset += read;
            }
            audioId = wasm.importAudioBytes(bytes);
            hash = wasmContentHash(bytes);
          }

          const info = wasm.audioInfo(audioId);
          const result = {
            id: audioId,
            duration: info.duration,
            sampleRate: info.sampleRate,
            channels: info.channels,
            name: message.name || info.name || undefined,
            hash,
            streamed
          };
          postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        } finally {
          if (!keepHandle) handle.close();
        }
        return;
      }
      case 'beginRecording': {
        const result = wasm.beginRecording(message.sampleRate, message.channels);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'appendSamples': {
        const samples = new Float32Array(message.samples);
        wasm.appendSamples(message.recordingId, samples);
        postMessage({ id: message.id, ok: true, result: undefined } satisfies ResponseMessage);
        return;
      }
      case 'finishRecording': {
        const finished = wasm.finishRecording(message.recordingId, message.name);
        const audioId = finished.audio;
        const source = finished.wav;
        const wav = new Uint8Array(source.length);
        wav.set(source);
        const hash = wasmContentHash(wav);
        const info = wasm.audioInfo(audioId);
        const result = {
          audioId,
          duration: info.duration,
          sampleRate: info.sampleRate,
          channels: info.channels,
          hash,
          wav
        };
        postMessage({ id: message.id, ok: true, result }, { transfer: [wav.buffer] });
        return;
      }
      case 'abortRecording': {
        wasm.abortRecording(message.recordingId);
        postMessage({ id: message.id, ok: true, result: undefined } satisfies ResponseMessage);
        return;
      }
      case 'waveformSlice': {
        const data = wasm.waveformSlice(message.audioId, message.t0, message.t1, message.px);
        const copy = new Float32Array(data.length);
        copy.set(data);
        postMessage(
          {
            id: message.id,
            ok: true,
            result: { t0: message.t0, t1: message.t1, px: message.px, data: copy }
          },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'samplesInRange': {
        const data = wasm.samplesInRange(message.audioId, message.t0, message.t1);
        const copy = new Float32Array(data.length);
        copy.set(data);
        postMessage(
          { id: message.id, ok: true, result: copy },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'spectrogramTile': {
        const req = message.req;
        // A custom ramp carries a 768-byte LUT and colorizes through the LUT
        // path; otherwise the named built-in colormap renders.
        const data = req.lut
          ? wasm.spectrogramTileRgbaLut(
              message.audioId,
              req.t0,
              req.t1,
              req.f0,
              req.f1,
              req.widthPx,
              req.heightPx,
              req.windowLength,
              req.maxFrequency,
              req.timeStep,
              req.frequencyStep,
              req.dynamicRangeDb,
              req.maxDb,
              req.lut,
              req.invert
            )
          : wasm.spectrogramTileRgba(
              message.audioId,
              req.t0,
              req.t1,
              req.f0,
              req.f1,
              req.widthPx,
              req.heightPx,
              req.windowLength,
              req.maxFrequency,
              req.timeStep,
              req.frequencyStep,
              req.dynamicRangeDb,
              req.maxDb,
              colormap(req.colormap),
              req.invert
            );
        const copy = new Uint8Array(data.length);
        copy.set(data);
        postMessage(
          { id: message.id, ok: true, result: { width: req.widthPx, height: req.heightPx, rgba: copy } },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'spectrogramProbe': {
        // Isolated engine timing for the perf report: the first colorize warms
        // the raw-dB block cache (STFT + colorize), the second re-colorizes the
        // same cached dB under a different palette (no STFT). The cached-block
        // count must not grow between them.
        const req = message.req;
        const run = (cm: SpectrogramTileRequest['colormap']) =>
          wasm.spectrogramTileRgba(
            message.audioId,
            req.t0,
            req.t1,
            req.f0,
            req.f1,
            req.widthPx,
            req.heightPx,
            req.windowLength,
            req.maxFrequency,
            req.timeStep,
            req.frequencyStep,
            req.dynamicRangeDb,
            req.maxDb,
            colormap(cm),
            req.invert
          );
        const cold0 = performance.now();
        run('Viridis');
        const stftMs = performance.now() - cold0;
        const blocksAfterStft = wasm.spectrogramCachedBlockCount();
        const warm0 = performance.now();
        run('Magma');
        const recolorMs = performance.now() - warm0;
        const blocksAfterRecolor = wasm.spectrogramCachedBlockCount();
        postMessage({
          id: message.id,
          ok: true,
          result: { stftMs, recolorMs, blocksAfterStft, blocksAfterRecolor }
        } satisfies ResponseMessage);
        return;
      }
      case 'pitchTrack': {
        const track = wasm.pitchTrack(message.audioId, message.floorHz, message.ceilingHz);
        const times = new Float64Array(track.times);
        const f0 = new Float64Array(track.f0);
        postMessage(
          { id: message.id, ok: true, result: { times, f0, maxHz: track.maxHz } },
          { transfer: [times.buffer, f0.buffer] }
        );
        return;
      }
      case 'pitchTrackSpan': {
        const track = wasm.pitchTrackSpan(
          message.audioId,
          message.floorHz,
          message.ceilingHz,
          message.t0,
          message.t1
        );
        const times = new Float64Array(track.times);
        const f0 = new Float64Array(track.f0);
        postMessage(
          { id: message.id, ok: true, result: { times, f0, maxHz: track.maxHz } },
          { transfer: [times.buffer, f0.buffer] }
        );
        return;
      }
      case 'formantTrack': {
        const track = wasm.formantTrack(
          message.audioId,
          message.ceilingHz,
          message.maxFormants,
          message.smoothed
        );
        const points = new Float64Array(track.points);
        postMessage(
          { id: message.id, ok: true, result: { points, maxHz: track.maxHz } },
          { transfer: [points.buffer] }
        );
        return;
      }
      case 'intensityTrack': {
        const track = wasm.intensityTrack(message.audioId, message.floorHz);
        const times = new Float64Array(track.times);
        const db = new Float64Array(track.db);
        postMessage(
          { id: message.id, ok: true, result: { times, db } },
          { transfer: [times.buffer, db.buffer] }
        );
        return;
      }
      case 'harmonicityTrack': {
        const track = wasm.harmonicityTrack(message.audioId, message.t0, message.t1);
        const times = new Float64Array(track.times);
        const hnr = new Float64Array(track.hnr);
        postMessage(
          { id: message.id, ok: true, result: { times, hnr } },
          { transfer: [times.buffer, hnr.buffer] }
        );
        return;
      }
      case 'cppTrack': {
        const track = wasm.cppTrack(message.audioId, message.t0, message.t1);
        const times = new Float64Array(track.times);
        const cpp = new Float64Array(track.cpp);
        postMessage(
          { id: message.id, ok: true, result: { times, cpp } },
          { transfer: [times.buffer, cpp.buffer] }
        );
        return;
      }
      case 'bandEnergy': {
        const result = wasm.bandEnergy(
          message.audioId,
          message.t0,
          message.t1,
          message.f0,
          message.f1
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'nearestZeroCrossing': {
        const result = wasm.nearestZeroCrossing(message.audioId, message.t);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'bandFilteredSpan': {
        const data = wasm.bandFilteredSpan(
          message.audioId,
          message.t0,
          message.t1,
          message.fLow,
          message.fHigh
        );
        const copy = new Float32Array(data.length);
        copy.set(data);
        postMessage(
          { id: message.id, ok: true, result: copy },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'selectionReadout': {
        const json = wasm.selectionReadout(
          message.audioId,
          message.t0,
          message.t1,
          message.f0,
          message.f1,
          message.pitchFloorHz,
          message.pitchCeilingHz,
          message.intensityFloorHz
        );
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) } satisfies ResponseMessage);
        return;
      }
      case 'formantSpanMeans': {
        const data = wasm.formantSpanMeans(
          message.audioId,
          message.ceilingHz,
          message.maxFormants,
          message.smoothed,
          message.t0,
          message.t1
        );
        const copy = new Float64Array(data.length);
        copy.set(data);
        postMessage(
          { id: message.id, ok: true, result: copy },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'pulseTimes': {
        const data = wasm.pulseTimes(message.audioId, message.floorHz, message.ceilingHz);
        const copy = new Float64Array(data.length);
        copy.set(data);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'spectrumSlice': {
        const json = wasm.spectrumSlice(message.audioId, message.t0, message.t1);
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) });
        return;
      }
      case 'lpcSpectrum': {
        const json = wasm.lpcSpectrum(message.audioId, message.t0, message.t1);
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) });
        return;
      }
      case 'ltas': {
        const json = wasm.ltas(message.audioId, message.t0, message.t1);
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) });
        return;
      }
      case 'silenceIntervals': {
        const json = wasm.silenceIntervals(
          message.audioId,
          message.thresholdDb,
          message.minSilentS,
          message.minSoundingS
        );
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) });
        return;
      }
      case 'voicingIntervals': {
        const json = wasm.voicingIntervals(
          message.audioId,
          message.pitchFloorHz,
          message.pitchCeilingHz,
          message.minVoicedS,
          message.minUnvoicedS
        );
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) });
        return;
      }
      case 'voiceReport': {
        const json = wasm.voiceReport(
          message.audioId,
          message.t0,
          message.t1,
          message.pitchFloorHz,
          message.pitchCeilingHz
        );
        postMessage({ id: message.id, ok: true, result: JSON.parse(json) } satisfies ResponseMessage);
        return;
      }
      case 'createAnnotation': {
        const result = wasm.createAnnotation(message.audioId, message.xmin, message.xmax);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'addIntervalTier': {
        const result = wasm.addIntervalTier(
          message.annotationId,
          message.name,
          WasmTierRelation.Independent,
          null
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'addPointTier': {
        const result = wasm.addPointTier(message.annotationId, message.name);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'renameTier': {
        const result = appliedToPlain(
          wasm.renameTier(message.annotationId, message.tierId, message.name)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'removeTier': {
        const result = appliedToPlain(wasm.removeTier(message.annotationId, message.tierId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'reorderTier': {
        const result = appliedToPlain(
          wasm.reorderTier(message.annotationId, message.tierId, message.toIndex)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'duplicateTier': {
        const result = appliedToPlain(wasm.duplicateTier(message.annotationId, message.tierId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'insertBoundary': {
        const result = wasm.insertBoundary(message.annotationId, message.tierId, message.at);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'insertPoint': {
        const result = wasm.insertPoint(
          message.annotationId,
          message.tierId,
          message.time,
          message.label
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'moveBoundary': {
        const result = appliedToPlain(
          wasm.moveBoundary(message.annotationId, message.boundaryId, message.to, message.linked)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'removeBoundary': {
        const result = appliedToPlain(wasm.removeBoundary(message.annotationId, message.boundaryId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'removePoint': {
        const result = appliedToPlain(wasm.removePoint(message.annotationId, message.pointId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'setIntervalLabel': {
        const result = appliedToPlain(
          wasm.setIntervalLabel(message.annotationId, message.tierId, message.intervalId, message.text)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'setPointLabel': {
        const result = appliedToPlain(
          wasm.setPointLabel(message.annotationId, message.tierId, message.pointId, message.text)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'renameAudio': {
        const result = appliedToPlain(wasm.renameAudio(message.audioId, message.name));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'detachAudio': {
        const result = appliedToPlain(wasm.detachAudio(message.audioId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'undo': {
        postMessage({ id: message.id, ok: true, result: appliedToPlain(wasm.undo()) } satisfies ResponseMessage);
        return;
      }
      case 'redo': {
        postMessage({ id: message.id, ok: true, result: appliedToPlain(wasm.redo()) } satisfies ResponseMessage);
        return;
      }
      case 'undoDepth': {
        postMessage({ id: message.id, ok: true, result: wasm.undoDepth() } satisfies ResponseMessage);
        return;
      }
      case 'redoDepth': {
        postMessage({ id: message.id, ok: true, result: wasm.redoDepth() } satisfies ResponseMessage);
        return;
      }
      case 'journalHeadId': {
        const head = wasm.journalHeadId();
        postMessage({ id: message.id, ok: true, result: head ?? null } satisfies ResponseMessage);
        return;
      }
      case 'stateHash': {
        postMessage({ id: message.id, ok: true, result: wasm.stateHash() } satisfies ResponseMessage);
        return;
      }
      case 'listAnnotations': {
        const result = Array.from(wasm.listAnnotations(message.audioId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'annotationTiers': {
        const result = tiersToPlain(wasm.annotationTiers(message.annotationId));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'intervalsInRange': {
        const result = intervalsToPlain(
          wasm.intervalsInRange(message.annotationId, message.tierId, message.t0, message.t1)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'pointsInRange': {
        const result = pointsToPlain(
          wasm.pointsInRange(message.annotationId, message.tierId, message.t0, message.t1)
        );
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'searchLabels': {
        const result = hitsToPlain(wasm.searchLabels(message.pattern, message.regex));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'importTextGrid': {
        const bytes = new Uint8Array(message.bytes);
        const result = wasm.importTextGrid(message.audioId, bytes);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'exportTextGrid': {
        const bytes = wasm.exportTextGrid(message.annotationId);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage(
          { id: message.id, ok: true, result: copy },
          { transfer: [copy.buffer] }
        );
        return;
      }
      case 'annotationJson': {
        const result = wasm.annotationJson(message.annotationId);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'attachAnnotationJson': {
        const result = wasm.attachAnnotationJson(message.audioId, message.json);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'saveProjectContainer': {
        const bytes = wasm.saveProjectContainer(message.specJson);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'saveProjectBundle': {
        const ids = BigUint64Array.from(message.ids.map((id) => BigInt(id)));
        const media = message.media.map((buffer) => new Uint8Array(buffer));
        const bytes = wasm.saveProjectBundle(message.specJson, ids, media);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'loadProjectContainer': {
        const bytes = new Uint8Array(message.bytes);
        const result = wasmLoadProjectContainer(bytes);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'readProjectBundle': {
        const bytes = new Uint8Array(message.bytes);
        const bundle = wasmReadProjectBundle(bytes);
        const ids = Array.from(bundle.embeddedIds, (id) => Number(id));
        const transfer: Transferable[] = [];
        const media = ids.map((_, index) => {
          const source = bundle.embeddedWav(index);
          const copy = new Uint8Array(source.length);
          copy.set(source);
          transfer.push(copy.buffer);
          return copy.buffer;
        });
        const result = { meta: bundle.meta, ids, media };
        postMessage({ id: message.id, ok: true, result }, { transfer });
        return;
      }
      case 'contentHash': {
        const result = wasmContentHash(new Uint8Array(message.bytes));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'exportSpanWav': {
        const bytes = wasm.exportSpanWav(message.audioId, message.t0, message.t1, WasmBitDepth[message.bits]);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'reverseSpanWav': {
        const bytes = wasm.reverseSpanWav(message.audioId, message.t0, message.t1, WasmBitDepth[message.bits]);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'scaleIntensitySpanWav': {
        const bytes = wasm.scaleIntensitySpanWav(
          message.audioId,
          message.t0,
          message.t1,
          message.targetDb,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'scalePeakSpanWav': {
        const bytes = wasm.scalePeakSpanWav(
          message.audioId,
          message.t0,
          message.t1,
          message.target,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'resampleWav': {
        const bytes = wasm.resampleWav(message.audioId, message.targetHz, WasmBitDepth[message.bits]);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'convertToMono': {
        const bytes = wasm.convertToMono(message.audioId, WasmBitDepth[message.bits]);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'concatWav': {
        const bytes = wasm.concatWav(new Float64Array(message.ids), WasmBitDepth[message.bits]);
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'exportBandFilteredSpanWav': {
        const bytes = wasm.exportBandFilteredSpanWav(
          message.audioId,
          message.t0,
          message.t1,
          message.fLow,
          message.fHigh,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'exportNotchFilteredSpanWav': {
        const bytes = wasm.exportNotchFilteredSpanWav(
          message.audioId,
          message.t0,
          message.t1,
          message.fLow,
          message.fHigh,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'exportChannelWav': {
        const bytes = wasm.exportChannelWav(
          message.audioId,
          message.channel,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'applyPreemphasisWav': {
        const bytes = wasm.applyPreemphasisWav(
          message.audioId,
          message.t0,
          message.t1,
          message.fromHz,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'subtractMeanWav': {
        const bytes = wasm.subtractMeanWav(
          message.audioId,
          message.t0,
          message.t1,
          WasmBitDepth[message.bits]
        );
        const copy = new Uint8Array(bytes.length);
        copy.set(bytes);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'renameProjectContainer': {
        const source = new Uint8Array(message.bytes);
        const renamed = wasmRenameProjectContainer(source, message.name);
        const copy = new Uint8Array(renamed.length);
        copy.set(renamed);
        postMessage({ id: message.id, ok: true, result: copy }, { transfer: [copy.buffer] });
        return;
      }
      case 'buildFigure': {
        const result = wasm.buildFigure(JSON.stringify(message.spec));
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'renderFigureSvg': {
        const result = wasmRenderFigureSvg(message.figureJson);
        postMessage({ id: message.id, ok: true, result } satisfies ResponseMessage);
        return;
      }
      case 'exportFigure': {
        const bundle = wasmExportFigure(message.figureJson, message.format);
        const mainSource = bundle.mainBytes;
        const mainBytes = new Uint8Array(mainSource.length);
        mainBytes.set(mainSource);
        const sidecarNames = bundle.sidecarNames;
        const sidecars = sidecarNames.map((name, index) => {
          const source = bundle.sidecarBytes(index);
          const bytes = new Uint8Array(source.length);
          bytes.set(source);
          return { name, bytes };
        });
        const result = {
          mainName: bundle.mainName,
          mainBytes,
          mime: bundle.mime,
          isText: bundle.isText,
          sidecars
        };
        const transfer = [mainBytes.buffer, ...sidecars.map((s) => s.bytes.buffer)];
        postMessage({ id: message.id, ok: true, result }, { transfer });
        return;
      }
      default: {
        const unexpected: never = message;
        const unknownId = (unexpected as { id: number }).id;
        postMessage({ id: unknownId, ok: false, error: 'unknown method' } satisfies ResponseMessage);
      }
    }
  } catch (error) {
    postMessage({
      id: message.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error)
    } satisfies ResponseMessage);
  }
};
