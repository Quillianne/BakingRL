export type GameEventFrame<TEvent extends string = string, TData = unknown> = {
  Event: TEvent;
  Data: TData;
};

export type RlTelemetryEventName =
  | "UpdateState"
  | "BallHit"
  | "ClockUpdatedSeconds"
  | "CountdownBegin"
  | "CrossbarHit"
  | "GoalReplayEnd"
  | "GoalReplayStart"
  | "GoalReplayWillEnd"
  | "GoalScored"
  | "MatchCreated"
  | "MatchInitialized"
  | "MatchDestroyed"
  | "MatchEnded"
  | "MatchPaused"
  | "MatchUnpaused"
  | "PodiumStart"
  | "ReplayCreated"
  | "ReplayWillEnd"
  | "RoundStarted"
  | "StatfeedEvent";

export type RlLocation = {
  X: number;
  Y: number;
  Z: number;
};

export type RlPlayerRef = {
  Name: string;
  Shortcut?: number;
  TeamNum: number;
};

export type RlPlayer = RlPlayerRef & {
  PrimaryId?: string;
  Score?: number;
  Goals?: number;
  Shots?: number;
  Assists?: number;
  Saves?: number;
  Touches?: number;
  CarTouches?: number;
  Demos?: number;
  bHasCar?: boolean;
  Speed?: number;
  Boost?: number;
  bBoosting?: boolean;
  bOnGround?: boolean;
  bOnWall?: boolean;
  bPowersliding?: boolean;
  bDemolished?: boolean;
  Attacker?: RlPlayerRef;
  bSupersonic?: boolean;
};

export type RlTeam = {
  Name: string;
  TeamNum: number;
  Score: number;
  ColorPrimary: string;
  ColorSecondary: string;
};

export type RlBallState = {
  Speed: number;
  TeamNum: number;
};

export type RlGameState = {
  Teams: RlTeam[];
  TimeSeconds: number;
  bOvertime: boolean;
  Frame?: number;
  Elapsed?: number;
  Ball: RlBallState;
  bReplay: boolean;
  bHasWinner: boolean;
  Winner: string;
  Arena: string;
  bHasTarget: boolean;
  Target?: RlPlayerRef;
};

export type RlUpdateStatePayload = {
  MatchGuid?: string;
  Players: RlPlayer[];
  Game: RlGameState;
};

export type RlBallHitPayload = {
  MatchGuid?: string;
  Players: RlPlayerRef[];
  Ball: {
    PreHitSpeed: number;
    PostHitSpeed: number;
    Location: RlLocation;
  };
};

export type RlClockUpdatedSecondsPayload = {
  MatchGuid?: string;
  TimeSeconds: number;
  bOvertime: boolean;
};

export type RlSimpleMatchPayload = {
  MatchGuid?: string;
};

export type RlBallLastTouch = {
  Player: RlPlayerRef;
  Speed: number;
};

export type RlCrossbarHitPayload = {
  MatchGuid?: string;
  BallLocation: RlLocation;
  BallSpeed: number;
  ImpactForce: number;
  BallLastTouch: RlBallLastTouch;
};

export type RlGoalScoredPayload = {
  MatchGuid?: string;
  GoalSpeed: number;
  GoalTime: number;
  ImpactLocation: RlLocation;
  Scorer: RlPlayerRef;
  Assister?: RlPlayerRef | null;
  BallLastTouch: RlBallLastTouch;
};

export type RlMatchEndedPayload = {
  MatchGuid?: string;
  WinnerTeamNum: number;
};

export type RlStatfeedEventPayload = {
  MatchGuid?: string;
  EventName: string;
  Type: string;
  MainTarget: RlPlayerRef;
  SecondaryTarget?: RlPlayerRef | null;
};

export type RlTelemetryPayloadByEvent = {
  UpdateState: RlUpdateStatePayload;
  BallHit: RlBallHitPayload;
  ClockUpdatedSeconds: RlClockUpdatedSecondsPayload;
  CountdownBegin: RlSimpleMatchPayload;
  CrossbarHit: RlCrossbarHitPayload;
  GoalReplayEnd: RlSimpleMatchPayload;
  GoalReplayStart: RlSimpleMatchPayload;
  GoalReplayWillEnd: RlSimpleMatchPayload;
  GoalScored: RlGoalScoredPayload;
  MatchCreated: RlSimpleMatchPayload;
  MatchInitialized: RlSimpleMatchPayload;
  MatchDestroyed: RlSimpleMatchPayload;
  MatchEnded: RlMatchEndedPayload;
  MatchPaused: RlSimpleMatchPayload;
  MatchUnpaused: RlSimpleMatchPayload;
  PodiumStart: RlSimpleMatchPayload;
  ReplayCreated: RlSimpleMatchPayload;
  ReplayWillEnd: RlSimpleMatchPayload;
  RoundStarted: RlSimpleMatchPayload;
  StatfeedEvent: RlStatfeedEventPayload;
};

export type RlTelemetryFrame<TEvent extends RlTelemetryEventName = RlTelemetryEventName> = {
  [Event in TEvent]: GameEventFrame<Event, RlTelemetryPayloadByEvent[Event]>;
}[TEvent];

type RlTelemetryTemplateMap = {
  [Event in RlTelemetryEventName]: GameEventFrame<Event, RlTelemetryPayloadByEvent[Event]>;
};

const MOCK_MATCH_GUID = "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6";
const BLUE_PLAYER: RlPlayerRef = { Name: "PlayerA", Shortcut: 1, TeamNum: 0 };
const ORANGE_PLAYER: RlPlayerRef = { Name: "PlayerB", Shortcut: 2, TeamNum: 1 };
const BLUE_ASSISTER: RlPlayerRef = { Name: "PlayerC", Shortcut: 3, TeamNum: 0 };

function simpleMatchPayload(): RlSimpleMatchPayload {
  return { MatchGuid: MOCK_MATCH_GUID };
}

function ballLastTouch(speed = 1250): RlBallLastTouch {
  return {
    Player: BLUE_PLAYER,
    Speed: speed
  };
}

export const RL_TELEMETRY_FRAME_TEMPLATES = {
  UpdateState: {
    Event: "UpdateState",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      Players: [
        {
          ...BLUE_PLAYER,
          PrimaryId: "Steam|123|0",
          Score: 125,
          Goals: 1,
          Shots: 2,
          Assists: 0,
          Saves: 1,
          Touches: 14,
          CarTouches: 3,
          Demos: 0,
          bHasCar: true,
          Speed: 1200,
          Boost: 45,
          bBoosting: true,
          bOnGround: true,
          bOnWall: false,
          bPowersliding: false,
          bDemolished: false,
          bSupersonic: true
        },
        {
          ...ORANGE_PLAYER,
          PrimaryId: "Steam|456|0",
          Score: 80,
          Goals: 0,
          Shots: 1,
          Assists: 0,
          Saves: 0,
          Touches: 9,
          CarTouches: 2,
          Demos: 0,
          bHasCar: true,
          Speed: 950,
          Boost: 72,
          bBoosting: false,
          bOnGround: true,
          bOnWall: false,
          bPowersliding: false,
          bDemolished: false,
          bSupersonic: false
        }
      ],
      Game: {
        Teams: [
          { Name: "Blue", TeamNum: 0, Score: 2, ColorPrimary: "0000FF", ColorSecondary: "0000AA" },
          { Name: "Orange", TeamNum: 1, Score: 1, ColorPrimary: "FF7A00", ColorSecondary: "AA4400" }
        ],
        TimeSeconds: 123,
        bOvertime: false,
        Frame: 120,
        Elapsed: 50.2,
        Ball: { Speed: 1200, TeamNum: 255 },
        bReplay: false,
        bHasWinner: false,
        Winner: "",
        Arena: "Stadium_P",
        bHasTarget: true,
        Target: BLUE_PLAYER
      }
    }
  },
  BallHit: {
    Event: "BallHit",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      Players: [BLUE_PLAYER],
      Ball: {
        PreHitSpeed: 800,
        PostHitSpeed: 1500,
        Location: { X: 0, Y: 0, Z: 92 }
      }
    }
  },
  ClockUpdatedSeconds: {
    Event: "ClockUpdatedSeconds",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      TimeSeconds: 180,
      bOvertime: false
    }
  },
  CountdownBegin: {
    Event: "CountdownBegin",
    Data: simpleMatchPayload()
  },
  CrossbarHit: {
    Event: "CrossbarHit",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      BallLocation: { X: 120, Y: -2944, Z: 320 },
      BallSpeed: 870.3,
      ImpactForce: 127.5,
      BallLastTouch: ballLastTouch(120)
    }
  },
  GoalReplayEnd: {
    Event: "GoalReplayEnd",
    Data: simpleMatchPayload()
  },
  GoalReplayStart: {
    Event: "GoalReplayStart",
    Data: simpleMatchPayload()
  },
  GoalReplayWillEnd: {
    Event: "GoalReplayWillEnd",
    Data: simpleMatchPayload()
  },
  GoalScored: {
    Event: "GoalScored",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      GoalSpeed: 1550,
      GoalTime: 221,
      ImpactLocation: { X: 0, Y: 5120, Z: 140 },
      Scorer: BLUE_PLAYER,
      Assister: BLUE_ASSISTER,
      BallLastTouch: ballLastTouch(1550)
    }
  },
  MatchCreated: {
    Event: "MatchCreated",
    Data: simpleMatchPayload()
  },
  MatchInitialized: {
    Event: "MatchInitialized",
    Data: simpleMatchPayload()
  },
  MatchDestroyed: {
    Event: "MatchDestroyed",
    Data: simpleMatchPayload()
  },
  MatchEnded: {
    Event: "MatchEnded",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      WinnerTeamNum: 0
    }
  },
  MatchPaused: {
    Event: "MatchPaused",
    Data: simpleMatchPayload()
  },
  MatchUnpaused: {
    Event: "MatchUnpaused",
    Data: simpleMatchPayload()
  },
  PodiumStart: {
    Event: "PodiumStart",
    Data: simpleMatchPayload()
  },
  ReplayCreated: {
    Event: "ReplayCreated",
    Data: simpleMatchPayload()
  },
  ReplayWillEnd: {
    Event: "ReplayWillEnd",
    Data: simpleMatchPayload()
  },
  RoundStarted: {
    Event: "RoundStarted",
    Data: simpleMatchPayload()
  },
  StatfeedEvent: {
    Event: "StatfeedEvent",
    Data: {
      MatchGuid: MOCK_MATCH_GUID,
      EventName: "Demolish",
      Type: "Demolition",
      MainTarget: BLUE_PLAYER,
      SecondaryTarget: ORANGE_PLAYER
    }
  }
} satisfies RlTelemetryTemplateMap;

export const RL_TELEMETRY_EVENT_NAMES = Object.keys(RL_TELEMETRY_FRAME_TEMPLATES) as RlTelemetryEventName[];

export function telemetryFrameTemplate<TEvent extends RlTelemetryEventName>(
  eventName: TEvent
): GameEventFrame<TEvent, RlTelemetryPayloadByEvent[TEvent]> {
  return JSON.parse(JSON.stringify(RL_TELEMETRY_FRAME_TEMPLATES[eventName]));
}

export function telemetryFrameTemplateJson(eventName: RlTelemetryEventName) {
  return JSON.stringify(telemetryFrameTemplate(eventName), null, 2);
}
