# Rocket League Stats API

## Summary

This document outlines the capabilities of the Rocket League Game Data API. First, players must ask the game to enable this feature by editing their `DefaultStatsAPI.ini`, explained below. Once active, this feature will open a web socket on the player's machine that emits gameplay data and events. Third-party programs can ingest this data to power a variety of applications, such as custom broadcaster HUDs.

## Overview

The Stats API broadcasts JSON messages over a local socket while a match is in progress. Messages are sent both at a configurable periodic rate and when specific match events occur. Event data is always emitted on the same tick that the event occurs, regardless of the user's `PacketSendRate`.

**Note:** All configuration must be done before the client starts — changes to the `.ini` file while the client is running require a restart.

**Field Visibility:**
* Fields marked **`CONDITIONAL`** are only present when relevant.
* Fields marked **`SPECTATOR`** are only present if the client is spectating or on the player's team.

---

## Configuration

Edit `<Install Dir>\TAGame\Config\DefaultStatsAPI.ini` before launching the client.

| Setting | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `PacketSendRate` | float | 0 (disabled) | Number of `UpdateState` packets broadcast per second. Must be > 0 to enable the websocket. Capped at 120. |
| `Port` | int | 49123 | Local port the socket listens on. |

---

## Message Format

Every message follows this envelope structure:

```json
{
  "Event": "EventName",
  "Data":  { /* event-specific payload */ }
}
```

---

## Tick (UpdateState)

Sent X amount of times per second based on the player's `PacketSendRate` preference.

### Example
```json
{
  "Event": "UpdateState",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "Players": [
      {
        "Name": "PlayerA",
        "PrimaryId": "Steam|123|0",
        "Shortcut": 1,
        "TeamNum": 0,
        "Score": 125,
        "Goals": 1,
        "Shots": 2,
        "Assists": 0,
        "Saves": 1,
        "Touches": 14,
        "CarTouches": 3,
        "Demos": 0,
        "bHasCar": true,
        "Speed": 1200,
        "Boost": 45,
        "bBoosting": true,
        "bOnGround": true,
        "bOnWall": false,
        "bPowersliding": false,
        "bDemolished": true,
        "Attacker": {
          "Name": "PlayerB",
          "Shortcut": 2,
          "TeamNum": 1
        },
        "bSupersonic": true
      }
    ],
    "Game": {
      "Teams": [
        {
          "Name": "Blue",
          "TeamNum": 0,
          "Score": 1,
          "ColorPrimary": "0000FF",
          "ColorSecondary": "0000AA"
        }
      ],
      "TimeSeconds": 180,
      "bOvertime": false,
      "Frame": 120,
      "Elapsed": 50.2,
      "Ball": {
        "Speed": 850.5,
        "TeamNum": 0
      },
      "bReplay": false,
      "bHasWinner": true,
      "Winner": "Blue",
      "Arena": "Stadium_P",
      "bHasTarget": true,
      "Target": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      }
    }
  }
}
```

### Data Payload Fields

| Field | Type | Description |
| :--- | :--- | :--- |
| `Players` | array | One entry per player in the match. |
| `Players[].Name` | string | Display name. |
| `Players[].PrimaryId` | string | Platform identifier in the format `Platform|Uid|Splitscreen` (e.g. "Steam\|123\|0"). |
| `Players[].Shortcut` | int | Spectator shortcut number. |
| `Players[].TeamNum` | int | Team index (0 = Blue, 1 = Orange). |
| `Players[].Score` | int | Total match score. |
| `Players[].Goals` | int | Goals scored this match. |
| `Players[].Shots` | int | Shot attempts this match. |
| `Players[].Assists` | int | Assists earned this match. |
| `Players[].Saves` | int | Saves made this match. |
| `Players[].Touches` | int | Total ball touches. |
| `Players[].CarTouches` | int | Touches by the car body (not ball). |
| `Players[].Demos` | int | Demolitions inflicted. |
| `Players[].bHasCar` | bool | **`SPECTATOR`** True if the player currently has a vehicle. |
| `Players[].Speed` | float | **`SPECTATOR`** Vehicle speed in Unreal Units/second. |
| `Players[].Boost` | int | **`SPECTATOR`** Boost amount 0–100. |
| `Players[].bBoosting` | bool | **`SPECTATOR`** True if the player is currently boosting. |
| `Players[].bOnGround` | bool | **`SPECTATOR`** True if at least 3 wheels are touching the world. |
| `Players[].bOnWall` | bool | **`SPECTATOR`** True if the vehicle is on a wall. |
| `Players[].bPowersliding` | bool | **`SPECTATOR`** True if the player is holding handbrake. |
| `Players[].bDemolished` | bool | **`SPECTATOR`** True if the vehicle is currently destroyed. |
| `Players[].bSupersonic` | bool | **`SPECTATOR`** True if the vehicle is at supersonic speed. |
| `Players[].Attacker` | object | **`CONDITIONAL`** The player who demolished this player. Present only when demolished. |
| `Players[].Attacker.Name` | string | Name of the player who demolished this player. |
| `Players[].Attacker.Shortcut`| int | Spectator shortcut of the attacker. |
| `Players[].Attacker.TeamNum` | int | Team index of the attacker. |
| `Game` | object | Match metadata. |
| `Game.Teams` | array | One entry per team, ordered by TeamNum. |
| `Game.Teams[].Name` | string | Team name. |
| `Game.Teams[].TeamNum` | int | Team index. |
| `Game.Teams[].Score` | int | Team goal count. |
| `Game.Teams[].ColorPrimary` | string | Hex color code (no #) for the team’s primary color. |
| `Game.Teams[].ColorSecondary`| string | Hex color code for the team’s secondary color. |
| `Game.TimeSeconds` | int | Seconds remaining in the match. |
| `Game.bOvertime` | bool | True if the match is in overtime. |
| `Game.Ball` | object | Current ball state. |
| `Game.Ball.Speed` | float | Current ball speed in Unreal Units/second. |
| `Game.Ball.TeamNum` | int | Index of the last team to touch the ball. 255 if the ball has not been touched. |
| `Game.bReplay` | bool | True if a goal replay or history replay is active. |
| `Game.bHasWinner` | bool | True if a team has won. |
| `Game.Winner` | string | Name of the winning team. Empty string if no winner yet. |
| `Game.Arena` | string | Asset name of the current map (e.g. "Stadium_P"). |
| `Game.bHasTarget` | bool | True if the client is currently viewing a specific vehicle. |
| `Game.Target` | object | **`CONDITIONAL`** Player currently being viewed. Empty/0 if no spectator target. |
| `Game.Target.Name` | string | Name of the player being viewed. |
| `Game.Target.Shortcut` | int | Spectator shortcut of the viewed player. |
| `Game.Target.TeamNum` | int | Team index of the viewed player. |
| `Game.Frame` | int | **`CONDITIONAL`** Current frame number if a replay is active. |
| `Game.Elapsed` | float | **`CONDITIONAL`** Seconds elapsed since game start if a replay is active. |

---

## Events

### BallHit
Sent one frame after the ball is hit.

```json
{
  "Event": "BallHit",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "Players": [
      {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      }
    ],
    "Ball": {
      "PreHitSpeed": 0,
      "PostHitSpeed": 1450.2,
      "Location": {
        "X": -512,
        "Y": 100,
        "Z": 200
      }
    }
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |
| `Players` | array | Players that hit the ball that frame. |
| `Players[].Name` | string | Display name. |
| `Players[].Shortcut`| int | Spectator shortcut. |
| `Players[].TeamNum` | int | Team index (0 = Blue, 1 = Orange). |
| `Ball` | object | Ball state at the moment of the hit. |
| `Ball.PreHitSpeed` | float | Ball speed before the hit (Unreal Units/second). |
| `Ball.PostHitSpeed`| float | Ball speed after the hit (Unreal Units/second). |
| `Ball.Location` | vector | World position (X, Y, Z) of the ball at impact. |

### ClockUpdatedSeconds
Sent when the in-game clock has changed.

```json
{
  "Event": "ClockUpdatedSeconds",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "TimeSeconds": 180,
    "bOvertime": false
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |
| `TimeSeconds` | int | Seconds remaining in the match. |
| `bOvertime` | bool | True if the game is in overtime. |

### CountdownBegin
Sent at the start of each round when the countdown starts.

```json
{
  "Event": "CountdownBegin",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |

### CrossbarHit
Sent when the ball hits a crossbar.

```json
{
  "Event": "CrossbarHit",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "BallLocation": {
      "X": 120,
      "Y": -2944,
      "Z": 320
    },
    "BallSpeed": 870.3,
    "ImpactForce": 127.5,
    "BallLastTouch": {
      "Player": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      },
      "Speed": 120
    }
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |
| `BallSpeed` | float | Ball speed on impact. |
| `ImpactForce` | float | Impact force of the ball relative to the crossbar normal. |
| `BallLocation`| vector | World position (X, Y, Z) of the ball when the impact occurred. |
| `BallLastTouch`| object | The last touch of the ball before the crossbar hit. |
| `BallLastTouch.Player` | object | The player who made the last touch. |
| `BallLastTouch.Player.Name` | string | Display name. |
| `BallLastTouch.Player.Shortcut` | int | Spectator shortcut. |
| `BallLastTouch.Player.TeamNum` | int | Team index (0 = Blue, 1 = Orange). |
| `BallLastTouch.Speed` | float | Speed of the ball resulting from this hit. |

### GoalReplayEnd
Sent when a goal replay ends.

```json
{
  "Event": "GoalReplayEnd",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### GoalReplayStart
Sent when a goal replay starts.

```json
{
  "Event": "GoalReplayStart",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### GoalReplayWillEnd
Sent when the ball explodes during a goal replay. If the replay is skipped this event will not fire.

```json
{
  "Event": "GoalReplayWillEnd",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### GoalScored
Sent when a goal is scored.

```json
{
  "Event": "GoalScored",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "GoalSpeed": 87.3,
    "GoalTime": 127.5,
    "ImpactLocation": {
      "X": 0,
      "Y": -2944,
      "Z": 320
    },
    "Scorer": {
      "Name": "PlayerA",
      "Shortcut": 1,
      "TeamNum": 0
    },
    "Assister": {
      "Name": "PlayerC",
      "Shortcut": 3,
      "TeamNum": 0
    },
    "BallLastTouch": {
      "Player": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      },
      "Speed": 125
    }
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |
| `GoalSpeed` | float | Speed of the ball (Unreal Units/second) when it crossed the goal line. |
| `GoalTime` | float | Length of the previous round in seconds. |
| `ImpactLocation`| vector | World position (X, Y, Z) of the ball when the goal was scored. |
| `Scorer` | object | The player who scored the goal. |
| `Scorer.Name` | string | Display name of the scorer. |
| `Scorer.Shortcut`| int | Spectator shortcut. |
| `Scorer.TeamNum` | int | Team index of the scorer. |
| `Assister` | object | **`CONDITIONAL`** Same shape as `Scorer`. Present only when an assist was recorded. |
| `BallLastTouch` | object | The last touch of the ball before the goal. |
| `BallLastTouch.Player` | object | The player who made the last touch. |
| `BallLastTouch.Speed` | float | Speed of the ball resulting from this touch. |

### MatchCreated
Sent when all teams are created and replicated.

```json
{
  "Event": "MatchCreated",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### MatchInitialized
Sent when the first countdown starts.

```json
{
  "Event": "MatchInitialized",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### MatchDestroyed
Sent when leaving the game.

```json
{
  "Event": "MatchDestroyed",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### MatchEnded
Sent when the match ends and a winner is chosen.

```json
{
  "Event": "MatchEnded",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "WinnerTeamNum": 0
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `WinnerTeamNum`| int | Team index of the winning team. |

### MatchPaused
Sent when the game is paused by a match admin.

```json
{
  "Event": "MatchPaused",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### MatchUnpaused
Sent when the game is unpaused by a match admin.

```json
{
  "Event": "MatchUnpaused",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### PodiumStart
Sent when the game enters the podium state after the match ends.

```json
{
  "Event": "PodiumStart",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### ReplayCreated
Sent when a replay is initialized. Does not pertain to goal replays, only replays you load via the Match History menu.

```json
{
  "Event": "ReplayCreated",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### ReplayWillEnd
Sent shortly before a replay ends.

```json
{
  "Event": "ReplayWillEnd",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### RoundStarted
Sent when the game enters the active state (after the countdown finishes).

```json
{
  "Event": "RoundStarted",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

### StatfeedEvent
Sent when someone earns a stat.

```json
{
  "Event": "StatfeedEvent",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "EventName": "Demolish",
    "Type": "Demolition",
    "MainTarget": {
      "Name": "PlayerA",
      "Shortcut": 1,
      "TeamNum": 0
    },
    "SecondaryTarget": {
      "Name": "PlayerB",
      "Shortcut": 2,
      "TeamNum": 1
    }
  }
}
```
| Field | Type | Description |
| :--- | :--- | :--- |
| `MatchGuid` | string | Only set for online or LAN matches. |
| `EventName` | string | Asset name of the StatEvent (e.g. "Demolish", "Save"). |
| `Type` | string | Localized display label for the stat (e.g. "Demolition"). |
| `MainTarget` | object | Player who earned the stat. |
| `MainTarget.Name` | string | Display name. |
| `MainTarget.Shortcut` | int | Spectator shortcut. |
| `MainTarget.TeamNum` | int | Team index (0 = Blue, 1 = Orange). |
| `SecondaryTarget`| object | **`CONDITIONAL`** Player involved in the stat (e.g. the demolished player). Same shape as `MainTarget`. |
