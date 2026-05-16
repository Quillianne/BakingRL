import type {
  GameEventFrame,
  RlBallHitPayload,
  RlClockUpdatedSecondsPayload,
  RlGoalScoredPayload,
  RlLocation,
  RlMatchEndedPayload,
  RlPlayer,
  RlPlayerRef,
  RlSimpleMatchPayload,
  RlStatfeedEventPayload,
  RlTeam,
  RlTelemetryEventName,
  RlTelemetryPayloadByEvent,
  RlUpdateStatePayload
} from "$lib/rlTelemetry";

export type MockTelemetrySequenceFrame = {
  atMs: number;
  frame: GameEventFrame;
};

export type MockTelemetryPerspective = "caster" | "player";

export type MockTelemetrySequence = {
  id: string;
  name: string;
  perspective: MockTelemetryPerspective;
  description: string;
  frames: MockTelemetrySequenceFrame[];
};

type MockTelemetrySequenceFixture = Omit<MockTelemetrySequence, "perspective">;

const MATCH_GUID = "MOCK-SEQUENCE-RL-MATCH-0001";
export const MOCK_SEQUENCE_UPDATE_STATE_FPS = 30;
const CASTER_EXCLUDED_SEQUENCE_IDS = new Set(["training-freeplay"]);
const HARD_STATE_BOUNDARY_EVENTS = new Set<RlTelemetryEventName>([
  "CountdownBegin",
  "GoalReplayEnd",
  "GoalReplayStart",
  "GoalReplayWillEnd",
  "GoalScored",
  "MatchCreated",
  "MatchDestroyed",
  "MatchEnded",
  "MatchInitialized",
  "PodiumStart",
  "ReplayWillEnd",
  "RoundStarted"
]);
const SMOOTH_NUMBER_KEYS = new Set(["Boost", "Elapsed", "Frame", "Speed", "TimeSeconds"]);

const BLUE_1: RlPlayerRef = { Name: "PlayerA", Shortcut: 1, TeamNum: 0 };
const BLUE_2: RlPlayerRef = { Name: "PlayerB", Shortcut: 2, TeamNum: 0 };
const ORANGE_1: RlPlayerRef = { Name: "PlayerC", Shortcut: 3, TeamNum: 1 };
const ORANGE_2: RlPlayerRef = { Name: "PlayerD", Shortcut: 4, TeamNum: 1 };
const PLAYER_POV_TARGET = BLUE_1;

const BLUE_TEAM: RlTeam = {
  Name: "Blue",
  TeamNum: 0,
  Score: 0,
  ColorPrimary: "006DFF",
  ColorSecondary: "003A8C"
};

const ORANGE_TEAM: RlTeam = {
  Name: "Orange",
  TeamNum: 1,
  Score: 0,
  ColorPrimary: "FF7A00",
  ColorSecondary: "9B3D00"
};

type PlayerOverrides = Partial<Omit<RlPlayer, "Name" | "Shortcut" | "TeamNum">>;

type UpdateOptions = {
  matchGuid?: string | null;
  timeSeconds: number;
  blueScore?: number;
  orangeScore?: number;
  frame?: number;
  elapsed?: number;
  ballSpeed?: number;
  ballTeam?: number;
  overtime?: boolean;
  replay?: boolean;
  target?: RlPlayerRef | null;
  winnerTeamNum?: number | null;
  arena?: string;
  players?: RlPlayer[];
  teams?: RlTeam[];
  playerOverrides?: Partial<Record<"blue1" | "blue2" | "orange1" | "orange2", PlayerOverrides>>;
};

function telemetryFrame<TEvent extends RlTelemetryEventName>(
  event: TEvent,
  data: RlTelemetryPayloadByEvent[TEvent]
): GameEventFrame<TEvent, RlTelemetryPayloadByEvent[TEvent]> {
  return { Event: event, Data: data };
}

function at<TEvent extends RlTelemetryEventName>(
  atMs: number,
  event: TEvent,
  data: RlTelemetryPayloadByEvent[TEvent]
): MockTelemetrySequenceFrame {
  return { atMs, frame: telemetryFrame(event, data) };
}

function optionalMatchGuid(matchGuid: string | null) {
  return matchGuid === null ? {} : { MatchGuid: matchGuid };
}

function simple(matchGuid: string | null = MATCH_GUID): RlSimpleMatchPayload {
  return optionalMatchGuid(matchGuid);
}

function loc(x: number, y: number, z: number): RlLocation {
  return { X: x, Y: y, Z: z };
}

function player(ref: RlPlayerRef, overrides: PlayerOverrides = {}): RlPlayer {
  return {
    ...ref,
    PrimaryId: `Mock|${ref.TeamNum}|${ref.Shortcut ?? ref.Name}`,
    Score: 0,
    Goals: 0,
    Shots: 0,
    Assists: 0,
    Saves: 0,
    Touches: 0,
    CarTouches: 0,
    Demos: 0,
    bHasCar: true,
    Speed: 0,
    Boost: 100,
    bBoosting: false,
    bOnGround: true,
    bOnWall: false,
    bPowersliding: false,
    bDemolished: false,
    bSupersonic: false,
    ...overrides
  };
}

function teams(blueScore = 0, orangeScore = 0): RlTeam[] {
  return [
    { ...BLUE_TEAM, Score: blueScore },
    { ...ORANGE_TEAM, Score: orangeScore }
  ];
}

function winnerName(winnerTeamNum: number | null | undefined) {
  if (winnerTeamNum === 0) return BLUE_TEAM.Name;
  if (winnerTeamNum === 1) return ORANGE_TEAM.Name;
  return "";
}

function updateState(options: UpdateOptions): RlUpdateStatePayload {
  const playerOverrides = options.playerOverrides ?? {};
  const target = options.target === undefined ? BLUE_1 : options.target;
  const winnerTeamNum = options.winnerTeamNum ?? null;
  const matchGuid = options.matchGuid === undefined ? MATCH_GUID : options.matchGuid;
  return {
    ...optionalMatchGuid(matchGuid),
    Players:
      options.players ??
      [
        player(BLUE_1, playerOverrides.blue1),
        player(BLUE_2, playerOverrides.blue2),
        player(ORANGE_1, playerOverrides.orange1),
        player(ORANGE_2, playerOverrides.orange2)
      ],
    Game: {
      Teams: options.teams ?? teams(options.blueScore ?? 0, options.orangeScore ?? 0),
      TimeSeconds: options.timeSeconds,
      bOvertime: options.overtime ?? false,
      Frame: options.frame ?? Math.max(0, Math.round((300 - options.timeSeconds) * 60)),
      Elapsed: options.elapsed ?? Math.max(0, 300 - options.timeSeconds),
      Ball: { Speed: options.ballSpeed ?? 0, TeamNum: options.ballTeam ?? 255 },
      bReplay: options.replay ?? false,
      bHasWinner: winnerTeamNum !== null,
      Winner: winnerName(winnerTeamNum),
      Arena: options.arena ?? "Stadium_P",
      bHasTarget: target !== null,
      Target: target ?? undefined
    }
  };
}

function soloPlayer(overrides: PlayerOverrides = {}) {
  return [player(BLUE_1, overrides)];
}

function ballHit(
  playerRef: RlPlayerRef,
  preHitSpeed: number,
  postHitSpeed: number,
  location: RlLocation,
  matchGuid: string | null = MATCH_GUID
): RlBallHitPayload {
  return {
    ...optionalMatchGuid(matchGuid),
    Players: [playerRef],
    Ball: {
      PreHitSpeed: preHitSpeed,
      PostHitSpeed: postHitSpeed,
      Location: location
    }
  };
}

function goalScored(
  scorer: RlPlayerRef,
  assister: RlPlayerRef | null,
  goalSpeed: number,
  goalTime: number,
  impactLocation: RlLocation,
  matchGuid: string | null = MATCH_GUID
): RlGoalScoredPayload {
  return {
    ...optionalMatchGuid(matchGuid),
    GoalSpeed: goalSpeed,
    GoalTime: goalTime,
    ImpactLocation: impactLocation,
    Scorer: scorer,
    Assister: assister,
    BallLastTouch: {
      Player: scorer,
      Speed: goalSpeed
    }
  };
}

function replayGoalScored(
  ballLastTouch: RlPlayerRef,
  impactLocation: RlLocation,
  ballLastTouchSpeed = 0,
  matchGuid: string | null = MATCH_GUID
): RlGoalScoredPayload {
  return {
    ...optionalMatchGuid(matchGuid),
    GoalSpeed: 0,
    GoalTime: 0,
    ImpactLocation: impactLocation,
    Scorer: { Name: "", Shortcut: 0, TeamNum: 0 },
    BallLastTouch: {
      Player: ballLastTouch,
      Speed: ballLastTouchSpeed
    }
  };
}

function statfeed(
  eventName: string,
  type: string,
  mainTarget: RlPlayerRef,
  secondaryTarget: RlPlayerRef | null = null,
  matchGuid: string | null = MATCH_GUID
): RlStatfeedEventPayload {
  return {
    ...optionalMatchGuid(matchGuid),
    EventName: eventName,
    Type: type,
    MainTarget: mainTarget,
    SecondaryTarget: secondaryTarget
  };
}

function matchEnded(winnerTeamNum: number): RlMatchEndedPayload {
  return {
    MatchGuid: MATCH_GUID,
    WinnerTeamNum: winnerTeamNum
  };
}

export function mockSequenceDurationMs(sequence: MockTelemetrySequence) {
  return sequence.frames.reduce((duration, frame) => Math.max(duration, frame.atMs), 0);
}

function isUpdateStateFrame(frame: GameEventFrame): frame is GameEventFrame<"UpdateState", RlUpdateStatePayload> {
  return frame.Event === "UpdateState";
}

function isClockUpdatedSecondsFrame(
  frame: GameEventFrame
): frame is GameEventFrame<"ClockUpdatedSeconds", RlClockUpdatedSecondsPayload> {
  return frame.Event === "ClockUpdatedSeconds";
}

function cloneValue<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function playerPovPlayer(playerData: RlPlayer): RlPlayer {
  const nextPlayer = { ...playerData };
  if (nextPlayer.TeamNum !== PLAYER_POV_TARGET.TeamNum) {
    delete nextPlayer.Boost;
    delete nextPlayer.bBoosting;
  }
  return nextPlayer;
}

function playerPovUpdateState(data: RlUpdateStatePayload): RlUpdateStatePayload {
  const nextData = cloneValue(data);
  nextData.Players = nextData.Players.map(playerPovPlayer);
  if (!nextData.Game.bReplay) {
    nextData.Game.bHasTarget = true;
    nextData.Game.Target = PLAYER_POV_TARGET;
  } else if (!nextData.Game.Target) {
    nextData.Game.bHasTarget = true;
    nextData.Game.Target = PLAYER_POV_TARGET;
  }
  return nextData;
}

function playerPovFrame(entry: MockTelemetrySequenceFrame): MockTelemetrySequenceFrame {
  if (!isUpdateStateFrame(entry.frame)) {
    return { atMs: entry.atMs, frame: cloneValue(entry.frame) };
  }

  return {
    atMs: entry.atMs,
    frame: telemetryFrame("UpdateState", playerPovUpdateState(entry.frame.Data))
  };
}

function sortSequenceFrames(frames: MockTelemetrySequenceFrame[]) {
  return [...frames].sort(
    (a, b) =>
      a.atMs - b.atMs ||
      Number(a.frame.Event === "UpdateState") - Number(b.frame.Event === "UpdateState")
  );
}

function normalizedUpdateStateFps(updateStateFps = MOCK_SEQUENCE_UPDATE_STATE_FPS) {
  return Math.max(1, Math.min(120, Math.round(updateStateFps)));
}

function smoothUpdateIntervalMs(updateStateFps = MOCK_SEQUENCE_UPDATE_STATE_FPS) {
  return 1000 / normalizedUpdateStateFps(updateStateFps);
}

function throttleFrameTime(index: number, intervalMs: number) {
  return Math.round(index * intervalMs);
}

function lerp(start: number, end: number, ratio: number) {
  return start + (end - start) * ratio;
}

function roundInterpolatedNumber(key: string, value: number) {
  if (key === "Elapsed" || key === "TimeSeconds") return Math.round(value * 10) / 10;
  if (SMOOTH_NUMBER_KEYS.has(key)) return Math.round(value);
  return value;
}

function interpolateValue(start: unknown, end: unknown, ratio: number, key = ""): unknown {
  if (typeof start === "number" && typeof end === "number") {
    return SMOOTH_NUMBER_KEYS.has(key) ? roundInterpolatedNumber(key, lerp(start, end, ratio)) : start;
  }

  if (Array.isArray(start) && Array.isArray(end)) {
    return end.map((entry, index) => interpolateValue(start[index], entry, ratio, key));
  }

  if (
    typeof start === "object" &&
    start !== null &&
    typeof end === "object" &&
    end !== null &&
    !Array.isArray(start) &&
    !Array.isArray(end)
  ) {
    const output: Record<string, unknown> = {};
    for (const entryKey of new Set([...Object.keys(start), ...Object.keys(end)])) {
      output[entryKey] = interpolateValue(
        (start as Record<string, unknown>)[entryKey],
        (end as Record<string, unknown>)[entryKey],
        ratio,
        entryKey
      );
    }
    return output;
  }

  return ratio < 1 ? start : end;
}

function interpolateUpdateState(
  start: RlUpdateStatePayload,
  end: RlUpdateStatePayload,
  ratio: number
): RlUpdateStatePayload {
  return interpolateValue(start, end, ratio) as RlUpdateStatePayload;
}

function hasHardBoundary(frames: MockTelemetrySequenceFrame[], startIndex: number, endIndex: number) {
  return frames
    .slice(startIndex + 1, endIndex)
    .some((entry) => HARD_STATE_BOUNDARY_EVENTS.has(entry.frame.Event as RlTelemetryEventName));
}

function clockGuid(data: RlClockUpdatedSecondsPayload | RlUpdateStatePayload) {
  return data.MatchGuid ?? "";
}

function clockSecond(timeSeconds: number) {
  return Math.max(0, Math.round(timeSeconds));
}

function hasClockUpdatedSeconds(
  frames: MockTelemetrySequenceFrame[],
  matchGuid: string,
  timeSeconds: number,
  fromMs: number,
  toMs: number
) {
  return frames.some((entry) => {
    if (!isClockUpdatedSecondsFrame(entry.frame)) return false;
    return (
      entry.atMs >= fromMs &&
      entry.atMs <= toMs &&
      clockGuid(entry.frame.Data) === matchGuid &&
      clockSecond(entry.frame.Data.TimeSeconds) === timeSeconds
    );
  });
}

function clockFrame(data: RlUpdateStatePayload, timeSeconds: number): GameEventFrame<"ClockUpdatedSeconds", RlClockUpdatedSecondsPayload> {
  return telemetryFrame("ClockUpdatedSeconds", {
    MatchGuid: data.MatchGuid,
    TimeSeconds: timeSeconds,
    bOvertime: data.Game.bOvertime
  });
}

function withGeneratedClockUpdates(frames: MockTelemetrySequenceFrame[]) {
  const sortedFrames = sortSequenceFrames(frames);
  const generated: MockTelemetrySequenceFrame[] = [];
  const generatedKeys = new Set<string>();
  const updateIndexes = sortedFrames
    .map((entry, index) => (isUpdateStateFrame(entry.frame) ? index : -1))
    .filter((index) => index >= 0);

  function pushClock(atMs: number, data: RlUpdateStatePayload, timeSeconds: number, fromMs: number, toMs: number) {
    const second = clockSecond(timeSeconds);
    const matchGuid = clockGuid(data);
    const key = `${atMs}:${matchGuid}:${second}`;
    if (generatedKeys.has(key)) return;
    if (hasClockUpdatedSeconds(sortedFrames, matchGuid, second, fromMs, toMs)) return;
    generatedKeys.add(key);
    generated.push({ atMs, frame: clockFrame(data, second) });
  }

  for (let index = 0; index < updateIndexes.length; index += 1) {
    const currentIndex = updateIndexes[index];
    const current = sortedFrames[currentIndex];
    if (!isUpdateStateFrame(current.frame)) continue;

    const nextIndex = updateIndexes[index + 1];
    if (nextIndex === undefined) continue;
    const next = sortedFrames[nextIndex];
    if (!isUpdateStateFrame(next.frame)) continue;
    if (hasHardBoundary(sortedFrames, currentIndex, nextIndex)) continue;

    const startSeconds = current.frame.Data.Game.TimeSeconds;
    const endSeconds = next.frame.Data.Game.TimeSeconds;
    if (!Number.isFinite(startSeconds) || !Number.isFinite(endSeconds) || startSeconds === endSeconds) continue;

    const gap = next.atMs - current.atMs;
    if (gap <= 0) continue;

    if (endSeconds < startSeconds) {
      for (let second = Math.ceil(startSeconds) - 1; second >= Math.ceil(endSeconds); second -= 1) {
        const ratio = (startSeconds - second) / (startSeconds - endSeconds);
        const atMs = Math.round(current.atMs + gap * ratio);
        pushClock(atMs, next.frame.Data, second, current.atMs, next.atMs);
      }
    } else {
      for (let second = Math.floor(startSeconds) + 1; second <= Math.floor(endSeconds); second += 1) {
        const ratio = (second - startSeconds) / (endSeconds - startSeconds);
        const atMs = Math.round(current.atMs + gap * ratio);
        pushClock(atMs, next.frame.Data, second, current.atMs, next.atMs);
      }
    }
  }

  return sortSequenceFrames([...sortedFrames, ...generated]);
}

function withSmoothedUpdateStates(frames: MockTelemetrySequenceFrame[], updateStateFps = MOCK_SEQUENCE_UPDATE_STATE_FPS) {
  const intervalMs = smoothUpdateIntervalMs(updateStateFps);
  const sortedFrames = sortSequenceFrames(frames);
  const generated: MockTelemetrySequenceFrame[] = [];
  const updateIndexes = sortedFrames
    .map((entry, index) => (isUpdateStateFrame(entry.frame) ? index : -1))
    .filter((index) => index >= 0);

  for (let index = 0; index < updateIndexes.length - 1; index += 1) {
    const startIndex = updateIndexes[index];
    const endIndex = updateIndexes[index + 1];
    const start = sortedFrames[startIndex];
    const end = sortedFrames[endIndex];
    if (!isUpdateStateFrame(start.frame) || !isUpdateStateFrame(end.frame)) continue;
    if (hasHardBoundary(sortedFrames, startIndex, endIndex)) continue;

    const gap = end.atMs - start.atMs;
    if (gap <= intervalMs * 1.5) continue;

    const startFrame = Math.floor(start.atMs / intervalMs) + 1;
    const endFrame = Math.ceil(end.atMs / intervalMs);

    for (let frameIndex = startFrame; frameIndex < endFrame; frameIndex += 1) {
      const atMs = throttleFrameTime(frameIndex, intervalMs);
      if (atMs <= start.atMs || atMs >= end.atMs) continue;
      const ratio = (atMs - start.atMs) / gap;
      generated.push({
        atMs,
        frame: telemetryFrame("UpdateState", interpolateUpdateState(start.frame.Data, end.frame.Data, ratio))
      });
    }
  }

  return sortSequenceFrames([...sortedFrames, ...generated]);
}

const MOCK_TELEMETRY_SEQUENCE_FIXTURES: MockTelemetrySequenceFixture[] = [
  {
    id: "join-game-countdown",
    name: "Join game + countdown",
    description: "Players arrive, teams fill, countdown starts, then the round begins.",
    frames: [
      at(0, "ClockUpdatedSeconds", { MatchGuid: "", TimeSeconds: 300, bOvertime: false }),
      at(120, "MatchCreated", simple("")),
      at(350, "MatchInitialized", simple()),
      at(700, "UpdateState", updateState({ timeSeconds: 300, blueScore: 0, orangeScore: 0, target: BLUE_1 })),
      at(1200, "UpdateState", updateState({
        timeSeconds: 300,
        blueScore: 0,
        orangeScore: 0,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Speed: 420, Boost: 100 },
          blue2: { Speed: 360, Boost: 100 },
          orange1: { Speed: 390, Boost: 100 },
          orange2: { Speed: 340, Boost: 100 }
        }
      })),
      at(1700, "CountdownBegin", simple()),
      at(4200, "RoundStarted", simple()),
      at(4550, "UpdateState", updateState({
        timeSeconds: 299,
        ballSpeed: 640,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Speed: 960, Boost: 94, bBoosting: true },
          orange1: { Speed: 980, Boost: 96, bBoosting: true }
        }
      })),
      at(5200, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 299, bOvertime: false })
    ]
  },
  {
    id: "live-play-touches-boost",
    name: "Live play - touches & boost",
    description: "A short gameplay exchange with ball hits, target changes and boost movement.",
    frames: [
      at(0, "UpdateState", updateState({
        timeSeconds: 244,
        blueScore: 1,
        orangeScore: 1,
        ballSpeed: 720,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 220, Touches: 9, Boost: 100, Speed: 850 },
          orange1: { Score: 180, Touches: 8, Boost: 64, Speed: 930 }
        }
      })),
      at(500, "BallHit", ballHit(ORANGE_1, 720, 1180, loc(220, -1700, 98))),
      at(1000, "UpdateState", updateState({
        timeSeconds: 243,
        blueScore: 1,
        orangeScore: 1,
        ballSpeed: 1180,
        ballTeam: 1,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 220, Touches: 9, Boost: 92, Speed: 1120, bBoosting: true },
          orange1: { Score: 190, Touches: 9, Boost: 58, Speed: 1060 }
        }
      })),
      at(1250, "BallHit", ballHit(BLUE_1, 1180, 1680, loc(-320, -280, 140))),
      at(1450, "UpdateState", updateState({
        timeSeconds: 243,
        blueScore: 1,
        orangeScore: 1,
        ballSpeed: 1680,
        ballTeam: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 230, Touches: 10, CarTouches: 1, Boost: 68, Speed: 1420, bBoosting: true, bSupersonic: true },
          orange1: { Score: 190, Touches: 9, Boost: 44, Speed: 920 }
        }
      })),
      at(2000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 242, bOvertime: false }),
      at(2000, "UpdateState", updateState({
        timeSeconds: 242,
        blueScore: 1,
        orangeScore: 1,
        ballSpeed: 940,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Score: 230, Touches: 10, Boost: 22, Speed: 1260 },
          blue2: { Score: 160, Touches: 7, Boost: 88, Speed: 1010 }
        }
      })),
      at(3000, "UpdateState", updateState({
        timeSeconds: 241,
        blueScore: 1,
        orangeScore: 1,
        ballSpeed: 760,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 230, Touches: 10, Boost: 100, Speed: 780 },
          blue2: { Score: 160, Touches: 7, Boost: 82, Speed: 940 }
        }
      }))
    ]
  },
  {
    id: "defensive-save",
    name: "Defensive save",
    description: "Opponent shot, aerial save, save statfeed and updated player stats.",
    frames: [
      at(0, "UpdateState", updateState({
        timeSeconds: 112,
        blueScore: 2,
        orangeScore: 2,
        ballSpeed: 990,
        ballTeam: 1,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 310, Saves: 0, Boost: 74, Speed: 940 },
          orange1: { Score: 270, Shots: 2, Boost: 52, Speed: 1110 }
        }
      })),
      at(400, "BallHit", ballHit(ORANGE_1, 990, 1580, loc(0, -4240, 310))),
      at(1000, "UpdateState", updateState({
        timeSeconds: 111,
        blueScore: 2,
        orangeScore: 2,
        ballSpeed: 1580,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 310, Saves: 0, Boost: 58, Speed: 1410, bBoosting: true, bOnGround: false },
          orange1: { Score: 280, Shots: 3, Boost: 45, Speed: 890 }
        }
      })),
      at(1200, "StatfeedEvent", statfeed("Save", "Save", BLUE_1, ORANGE_1)),
      at(1450, "BallHit", ballHit(BLUE_1, 1580, 1450, loc(-180, -4930, 420))),
      at(1800, "UpdateState", updateState({
        timeSeconds: 111,
        blueScore: 2,
        orangeScore: 2,
        ballSpeed: 1450,
        ballTeam: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 360, Saves: 1, Touches: 12, Boost: 36, Speed: 1200, bOnGround: false },
          orange1: { Score: 280, Shots: 3, Boost: 42, Speed: 810 }
        }
      })),
      at(2000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 110, bOvertime: false }),
      at(2000, "UpdateState", updateState({
        timeSeconds: 110,
        blueScore: 2,
        orangeScore: 2,
        ballSpeed: 1380,
        ballTeam: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 360, Saves: 1, Touches: 12, Boost: 34, Speed: 1080, bOnGround: false },
          orange1: { Score: 280, Shots: 3, Boost: 41, Speed: 760 }
        }
      }))
    ]
  },
  {
    id: "goal-replay-countdown",
    name: "Goal + replay + next countdown",
    description: "Goal statfeed, goal event, GoalReplay lifecycle, then a new countdown begins.",
    frames: [
      at(0, "CountdownBegin", simple()),
      at(300, "RoundStarted", simple()),
      at(600, "UpdateState", updateState({
        timeSeconds: 76,
        blueScore: 2,
        orangeScore: 2,
        ballSpeed: 1180,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Shots: 3, Boost: 64 },
          blue2: { Score: 310, Assists: 1, Boost: 70 }
        }
      })),
      at(900, "BallHit", ballHit(BLUE_2, 1180, 1320, loc(-650, 1800, 180))),
      at(1200, "StatfeedEvent", statfeed("Goal", "Goal", BLUE_1, BLUE_2)),
      at(1250, "StatfeedEvent", statfeed("Assist", "Assist", BLUE_2, BLUE_1)),
      at(1350, "GoalScored", goalScored(BLUE_1, BLUE_2, 1550, 76, loc(0, 5120, 140))),
      at(1800, "UpdateState", updateState({
        timeSeconds: 76,
        blueScore: 3,
        orangeScore: 2,
        ballSpeed: 520,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 520, Goals: 2, Shots: 4, Boost: 64 },
          blue2: { Score: 360, Assists: 2, Boost: 70 }
        }
      })),
      at(2200, "GoalReplayStart", simple()),
      at(2400, "UpdateState", updateState({
        timeSeconds: 76,
        blueScore: 3,
        orangeScore: 2,
        ballSpeed: 320,
        replay: true,
        target: null,
        playerOverrides: {
          blue1: { Score: 520, Goals: 2, Shots: 4, Boost: 64 },
          blue2: { Score: 360, Assists: 2, Boost: 70 }
        }
      })),
      at(4200, "GoalReplayWillEnd", simple()),
      at(4950, "GoalScored", replayGoalScored(BLUE_1, loc(0, 5120, 140), 1550)),
      at(5000, "GoalReplayEnd", simple()),
      at(5050, "CountdownBegin", simple()),
      at(5600, "UpdateState", updateState({
        timeSeconds: 76,
        blueScore: 3,
        orangeScore: 2,
        ballSpeed: 0,
        replay: false,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 520, Goals: 2, Shots: 4, Boost: 100 },
          blue2: { Score: 360, Assists: 2, Boost: 100 }
        }
      })),
      at(6500, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 76, bOvertime: false }),
      at(7300, "RoundStarted", simple()),
      at(7600, "UpdateState", updateState({
        timeSeconds: 76,
        blueScore: 3,
        orangeScore: 2,
        ballSpeed: 0,
        replay: false,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 520, Goals: 2, Shots: 4, Boost: 100 },
          blue2: { Score: 360, Assists: 2, Boost: 100 }
        }
      }))
    ]
  },
  {
    id: "full-game",
    name: "Full game",
    description: "Join, kickoff, save, demolition, goal replay, next countdown, late play and match end.",
    frames: [
      at(0, "MatchCreated", simple()),
      at(350, "MatchInitialized", simple()),
      at(700, "UpdateState", updateState({
        timeSeconds: 30,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 0, Boost: 100, Speed: 0 },
          blue2: { Score: 0, Boost: 100, Speed: 0 },
          orange1: { Score: 0, Boost: 100, Speed: 0 },
          orange2: { Score: 0, Boost: 100, Speed: 0 }
        }
      })),
      at(1200, "UpdateState", updateState({
        timeSeconds: 30,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 0,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Boost: 100, Speed: 360 },
          blue2: { Boost: 100, Speed: 340 },
          orange1: { Boost: 100, Speed: 390 },
          orange2: { Boost: 100, Speed: 350 }
        }
      })),
      at(1700, "CountdownBegin", simple()),
      at(2600, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 30, bOvertime: false }),
      at(3600, "RoundStarted", simple()),
      at(4000, "UpdateState", updateState({
        timeSeconds: 30,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 620,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 0, Boost: 94, Speed: 980, bBoosting: true },
          orange1: { Score: 0, Boost: 96, Speed: 1020, bBoosting: true }
        }
      })),
      at(5000, "UpdateState", updateState({
        timeSeconds: 29,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 980,
        ballTeam: 1,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 0, Boost: 82, Speed: 1200, bBoosting: true },
          orange1: { Score: 0, Boost: 84, Speed: 1360, bBoosting: true }
        }
      })),
      at(6000, "BallHit", ballHit(ORANGE_1, 620, 1520, loc(0, -4240, 280))),
      at(7000, "UpdateState", updateState({
        timeSeconds: 27,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1520,
        ballTeam: 1,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 0, Saves: 0, Boost: 62, Speed: 1420, bBoosting: true, bOnGround: false },
          orange1: { Score: 10, Shots: 1, Boost: 72, Speed: 1040 }
        }
      })),
      at(7200, "StatfeedEvent", statfeed("Save", "Save", BLUE_1, ORANGE_1)),
      at(7500, "BallHit", ballHit(BLUE_1, 1520, 1320, loc(-180, -4930, 420))),
      at(9000, "UpdateState", updateState({
        timeSeconds: 25,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1320,
        ballTeam: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 50, Saves: 1, Touches: 1, Boost: 42, Speed: 980 },
          blue2: { Score: 0, Boost: 86, Speed: 1080 },
          orange1: { Score: 10, Shots: 1, Boost: 64, Speed: 860 }
        }
      })),
      at(10000, "UpdateState", updateState({
        timeSeconds: 24,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1320,
        ballTeam: 0,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Score: 50, Saves: 1, Touches: 1, Boost: 36, Speed: 1160, bOnGround: false },
          blue2: { Score: 0, Boost: 78, Speed: 1280, bBoosting: true },
          orange1: { Score: 10, Shots: 1, Boost: 58, Speed: 910 }
        }
      })),
      at(11000, "UpdateState", updateState({
        timeSeconds: 23,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1240,
        ballTeam: 0,
        target: ORANGE_2,
        playerOverrides: {
          blue1: { Score: 50, Saves: 1, Touches: 1, Boost: 34, Speed: 1080 },
          blue2: { Score: 0, Boost: 62, Speed: 1360, bBoosting: true },
          orange2: { Score: 0, Boost: 28, Speed: 1180 }
        }
      })),
      at(11800, "StatfeedEvent", statfeed("Demolish", "Demolition", BLUE_2, ORANGE_2)),
      at(12000, "UpdateState", updateState({
        timeSeconds: 22,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1180,
        ballTeam: 0,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Score: 50, Saves: 1, Touches: 1, Boost: 32, Speed: 1040 },
          blue2: { Score: 20, Demos: 1, Boost: 48, Speed: 1420, bSupersonic: true },
          orange2: { bDemolished: true, Boost: 0, Speed: 0, Attacker: BLUE_2 }
        }
      })),
      at(13000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 21, bOvertime: false }),
      at(14000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 20, bOvertime: false }),
      at(14500, "BallHit", ballHit(BLUE_2, 1180, 1560, loc(-650, 1800, 180))),
      at(15000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 19, bOvertime: false }),
      at(15000, "UpdateState", updateState({
        timeSeconds: 19,
        blueScore: 0,
        orangeScore: 0,
        ballSpeed: 1560,
        ballTeam: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 50, Saves: 1, Touches: 1, Shots: 1, Boost: 30, Speed: 1480, bBoosting: true },
          blue2: { Score: 20, Demos: 1, Boost: 38, Speed: 1260 },
          orange2: { bDemolished: false, Boost: 100, Speed: 620 }
        }
      })),
      at(16000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 18, bOvertime: false }),
      at(16100, "StatfeedEvent", statfeed("Goal", "Goal", BLUE_1, BLUE_2)),
      at(16200, "StatfeedEvent", statfeed("Assist", "Assist", BLUE_2, BLUE_1)),
      at(16300, "GoalScored", goalScored(BLUE_1, BLUE_2, 1560, 18, loc(0, 5120, 140))),
      at(17000, "UpdateState", updateState({
        timeSeconds: 18,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 460,
        replay: false,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Shots: 1, Saves: 1, Touches: 1, Boost: 32 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 48 },
          orange2: { bDemolished: false, Boost: 100, Speed: 0 }
        }
      })),
      at(17300, "GoalReplayStart", simple()),
      at(17600, "UpdateState", updateState({
        timeSeconds: 18,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 280,
        replay: true,
        target: null,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Shots: 1, Saves: 1, Touches: 1, Boost: 32 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 48 },
          orange2: { bDemolished: false, Boost: 100, Speed: 0 }
        }
      })),
      at(18000, "UpdateState", updateState({
        timeSeconds: 18,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 240,
        replay: true,
        target: null,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Shots: 1, Saves: 1, Touches: 1, Boost: 32 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 48 },
          orange2: { bDemolished: false, Boost: 100, Speed: 0 }
        }
      })),
      at(19000, "UpdateState", updateState({
        timeSeconds: 18,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 180,
        replay: true,
        target: null,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Shots: 1, Saves: 1, Touches: 1, Boost: 32 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 48 },
          orange2: { bDemolished: false, Boost: 100, Speed: 0 }
        }
      })),
      at(20000, "GoalReplayWillEnd", simple()),
      at(20900, "GoalScored", replayGoalScored(BLUE_1, loc(0, 5120, 140), 1560)),
      at(21000, "GoalReplayEnd", simple()),
      at(21600, "CountdownBegin", simple()),
      at(22600, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 18, bOvertime: false }),
      at(23400, "RoundStarted", simple()),
      at(23800, "UpdateState", updateState({
        timeSeconds: 18,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 0,
        replay: false,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Saves: 1, Boost: 100, Speed: 940 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 100, Speed: 920 },
          orange1: { Boost: 100, Speed: 960 },
          orange2: { Boost: 100, Speed: 910 }
        }
      })),
      at(25000, "UpdateState", updateState({
        timeSeconds: 17,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 760,
        ballTeam: 1,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Saves: 1, Boost: 82, Speed: 1120 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 92, Speed: 960 },
          orange1: { Score: 20, Shots: 1, Boost: 78, Speed: 1340, bBoosting: true }
        }
      })),
      at(27000, "BallHit", ballHit(ORANGE_1, 860, 1180, loc(340, -1200, 110))),
      at(30000, "UpdateState", updateState({
        timeSeconds: 12,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 1180,
        ballTeam: 1,
        target: BLUE_2,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Saves: 1, Boost: 62, Speed: 1040 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 86, Speed: 1280, bBoosting: true },
          orange1: { Score: 20, Shots: 1, Boost: 58, Speed: 1040 }
        }
      })),
      at(35800, "UpdateState", updateState({
        timeSeconds: 6,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 620,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Saves: 1, Boost: 44, Speed: 1280, bBoosting: true },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 74, Speed: 1040 },
          orange1: { Score: 20, Shots: 1, Boost: 46, Speed: 1160 }
        }
      })),
      at(41800, "UpdateState", updateState({
        timeSeconds: 0,
        blueScore: 1,
        orangeScore: 0,
        ballSpeed: 0,
        target: null,
        winnerTeamNum: 0,
        playerOverrides: {
          blue1: { Score: 150, Goals: 1, Saves: 1, Boost: 44, Speed: 0 },
          blue2: { Score: 70, Assists: 1, Demos: 1, Boost: 74, Speed: 0 }
        }
      })),
      at(42000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 0, bOvertime: false }),
      at(42100, "MatchEnded", matchEnded(0)),
      at(43000, "PodiumStart", simple()),
      at(44300, "MatchDestroyed", simple())
    ]
  },
  {
    id: "blue-won",
    name: "Blue won",
    description: "Final seconds, Blue wins, match end event and podium start.",
    frames: [
      at(0, "UpdateState", updateState({ timeSeconds: 10, blueScore: 4, orangeScore: 3, ballSpeed: 840, target: BLUE_1 })),
      at(1000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 9, bOvertime: false }),
      at(2000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 8, bOvertime: false }),
      at(7000, "UpdateState", updateState({ timeSeconds: 3, blueScore: 4, orangeScore: 3, ballSpeed: 420, target: BLUE_2 })),
      at(10000, "UpdateState", updateState({ timeSeconds: 0, blueScore: 4, orangeScore: 3, ballSpeed: 0, target: null, winnerTeamNum: 0 })),
      at(10200, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 0, bOvertime: false }),
      at(10300, "MatchEnded", matchEnded(0)),
      at(11200, "PodiumStart", simple()),
      at(12500, "MatchDestroyed", simple())
    ]
  },
  {
    id: "orange-won",
    name: "Orange won",
    description: "Final seconds, Orange wins, match end event and podium start.",
    frames: [
      at(0, "UpdateState", updateState({ timeSeconds: 10, blueScore: 2, orangeScore: 3, ballSpeed: 920, target: ORANGE_2 })),
      at(1000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 9, bOvertime: false }),
      at(2000, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 8, bOvertime: false }),
      at(7000, "UpdateState", updateState({ timeSeconds: 3, blueScore: 2, orangeScore: 3, ballSpeed: 310, target: BLUE_1 })),
      at(10000, "UpdateState", updateState({ timeSeconds: 0, blueScore: 2, orangeScore: 3, ballSpeed: 0, target: null, winnerTeamNum: 1 })),
      at(10200, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 0, bOvertime: false }),
      at(10300, "MatchEnded", matchEnded(1)),
      at(11200, "PodiumStart", simple()),
      at(12500, "MatchDestroyed", simple())
    ]
  },
  {
    id: "overtime-goal-win",
    name: "Overtime goal win",
    description: "Regulation ends tied, overtime begins, Orange scores, replay winds down, then match end and podium fire.",
    frames: [
      at(0, "UpdateState", updateState({ timeSeconds: 0, blueScore: 2, orangeScore: 2, ballSpeed: 640, target: BLUE_1 })),
      at(300, "CountdownBegin", simple()),
      at(650, "UpdateState", updateState({
        timeSeconds: 0,
        blueScore: 2,
        orangeScore: 2,
        overtime: true,
        ballSpeed: 0,
        target: BLUE_1,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Boost: 100, Speed: 0 },
          orange1: { Score: 480, Goals: 1, Boost: 100, Speed: 0 }
        }
      })),
      at(1300, "RoundStarted", simple()),
      at(1800, "UpdateState", updateState({
        timeSeconds: 0,
        blueScore: 2,
        orangeScore: 2,
        overtime: true,
        ballSpeed: 780,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Boost: 82, Speed: 1180 },
          orange1: { Score: 480, Goals: 1, Boost: 76, Speed: 1300, bBoosting: true }
        }
      })),
      at(2800, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 1, bOvertime: true }),
      at(3800, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 2, bOvertime: true }),
      at(4800, "BallHit", ballHit(ORANGE_2, 920, 1420, loc(220, -4600, 220))),
      at(5200, "StatfeedEvent", statfeed("Goal", "Goal", ORANGE_1, ORANGE_2)),
      at(5250, "StatfeedEvent", statfeed("OvertimeGoal", "Overtime Goal", ORANGE_1)),
      at(5300, "StatfeedEvent", statfeed("Assist", "Assist", ORANGE_2, ORANGE_1)),
      at(5400, "GoalScored", goalScored(ORANGE_1, ORANGE_2, 1420, 2, loc(0, -5120, 140))),
      at(5900, "UpdateState", updateState({
        timeSeconds: 2,
        blueScore: 2,
        orangeScore: 3,
        overtime: true,
        ballSpeed: 420,
        target: ORANGE_1,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Boost: 64, Speed: 820 },
          orange1: { Score: 580, Goals: 2, Boost: 44, Speed: 980 },
          orange2: { Score: 340, Assists: 1, Boost: 52, Speed: 760 }
        }
      })),
      at(7100, "GoalReplayStart", simple()),
      at(7600, "UpdateState", updateState({
        timeSeconds: 2,
        blueScore: 2,
        orangeScore: 3,
        overtime: true,
        ballSpeed: 260,
        replay: true,
        target: null,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Boost: 64, Speed: 0 },
          orange1: { Score: 580, Goals: 2, Boost: 44, Speed: 0 },
          orange2: { Score: 340, Assists: 1, Boost: 52, Speed: 0 }
        }
      })),
      at(9000, "GoalReplayWillEnd", simple()),
      at(9600, "StatfeedEvent", statfeed("Win", "Win", ORANGE_1)),
      at(9650, "StatfeedEvent", statfeed("MVP", "MVP", ORANGE_1)),
      at(9680, "GoalScored", replayGoalScored(ORANGE_1, loc(0, -5120, 140), 1420)),
      at(9700, "GoalReplayEnd", simple()),
      at(9750, "ClockUpdatedSeconds", { MatchGuid: MATCH_GUID, TimeSeconds: 0, bOvertime: true }),
      at(9800, "MatchEnded", matchEnded(1)),
      at(10000, "UpdateState", updateState({
        timeSeconds: 0,
        blueScore: 2,
        orangeScore: 3,
        overtime: true,
        ballSpeed: 0,
        target: null,
        winnerTeamNum: 1,
        playerOverrides: {
          blue1: { Score: 420, Goals: 1, Boost: 64, Speed: 0 },
          orange1: { Score: 580, Goals: 2, Boost: 44, Speed: 0 },
          orange2: { Score: 340, Assists: 1, Boost: 52, Speed: 0 }
        }
      })),
      at(10800, "PodiumStart", simple()),
      at(12000, "MatchDestroyed", simple())
    ]
  },
  {
    id: "training-freeplay",
    name: "Training freeplay",
    description: "Solo freeplay-like state with touches, boost reset, shot and local goal reset.",
    frames: [
      at(0, "MatchInitialized", simple(null)),
      at(250, "RoundStarted", simple(null)),
      at(650, "UpdateState", updateState({
        matchGuid: null,
        timeSeconds: 0,
        teams: [],
        players: soloPlayer({ Score: 0, Touches: 0, Boost: 100, Speed: 0 }),
        ballSpeed: 0,
        target: BLUE_1,
        arena: "Park_P"
      })),
      at(1500, "BallHit", ballHit(BLUE_1, 0, 920, loc(0, 0, 92), null)),
      at(1900, "UpdateState", updateState({
        matchGuid: null,
        timeSeconds: 0,
        teams: [],
        players: soloPlayer({ Score: 10, Touches: 1, Boost: 82, Speed: 1180, bBoosting: true }),
        ballSpeed: 920,
        target: BLUE_1,
        arena: "Park_P"
      })),
      at(2700, "BallHit", ballHit(BLUE_1, 920, 1480, loc(320, 2100, 260), null)),
      at(3300, "UpdateState", updateState({
        matchGuid: null,
        timeSeconds: 0,
        teams: [],
        players: soloPlayer({ Score: 20, Touches: 2, Boost: 100, Speed: 940 }),
        ballSpeed: 1480,
        target: BLUE_1,
        arena: "Park_P"
      })),
      at(4300, "GoalScored", goalScored(BLUE_1, null, 1480, 0, loc(0, 5120, 120), null)),
      at(5000, "StatfeedEvent", statfeed("Shot", "Shot", BLUE_1, null, null)),
      at(5650, "RoundStarted", simple(null)),
      at(6250, "UpdateState", updateState({
        matchGuid: null,
        timeSeconds: 0,
        teams: [],
        players: soloPlayer({ Score: 20, Touches: 2, Boost: 100, Speed: 0 }),
        ballSpeed: 0,
        target: BLUE_1,
        arena: "Park_P"
      }))
    ]
  }
];

function sequenceVariantDescription(sequence: MockTelemetrySequenceFixture, perspective: MockTelemetryPerspective) {
  if (perspective === "caster") return sequence.description;
  if (sequence.id === "blue-won") {
    return "Final seconds, PlayerA wins, match end event and podium start. Fixed PlayerA POV outside replay; opponent boost is omitted.";
  }
  if (sequence.id === "orange-won") {
    return "Final seconds, PlayerA loses, match end event and podium start. Fixed PlayerA POV outside replay; opponent boost is omitted.";
  }
  return `${sequence.description} Fixed PlayerA POV outside replay; opponent boost is omitted.`;
}

function sequenceVariantName(sequence: MockTelemetrySequenceFixture, perspective: MockTelemetryPerspective) {
  if (perspective === "player" && sequence.id === "blue-won") return "Game won";
  if (perspective === "player" && sequence.id === "orange-won") return "Game lost";
  return sequence.name;
}

function sequenceVariantFrames(sequence: MockTelemetrySequenceFixture, perspective: MockTelemetryPerspective) {
  return perspective === "player" ? sequence.frames.map(playerPovFrame) : sequence.frames;
}

function mockTelemetrySequenceVariant(
  sequence: MockTelemetrySequenceFixture,
  perspective: MockTelemetryPerspective,
  updateStateFps: number
): MockTelemetrySequence {
  return {
    ...sequence,
    id: `${perspective}-${sequence.id}`,
    name: sequenceVariantName(sequence, perspective),
    perspective,
    description: sequenceVariantDescription(sequence, perspective),
    frames: withSmoothedUpdateStates(withGeneratedClockUpdates(sequenceVariantFrames(sequence, perspective)), updateStateFps)
  };
}

export function mockTelemetrySequences(updateStateFps = MOCK_SEQUENCE_UPDATE_STATE_FPS): MockTelemetrySequence[] {
  return MOCK_TELEMETRY_SEQUENCE_FIXTURES.flatMap((sequence) => {
    const variants = [mockTelemetrySequenceVariant(sequence, "player", updateStateFps)];
    if (!CASTER_EXCLUDED_SEQUENCE_IDS.has(sequence.id)) {
      variants.unshift(mockTelemetrySequenceVariant(sequence, "caster", updateStateFps));
    }
    return variants;
  });
}

export const MOCK_TELEMETRY_SEQUENCES: MockTelemetrySequence[] = mockTelemetrySequences();
