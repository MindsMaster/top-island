// 注入到网易云渲染进程（orpheus:// 主帧），从 NCM 原生事件跟踪播放状态，
// 通过本地 WebSocket 上报给 top-island。
//
// NCM 3.1 事件模型（3.1.39 实测）：
//   audioplayer.load(playId, ...)            call，比 onLoad 早几十毫秒
//   audioplayer.onLoad(playId, {duration})   秒
//   audioplayer.onPlayState(playId, tag, s)  1 播放，2 暂停
//   audioplayer.onPlayProgress(playId, sec, buffered)   第三参是缓冲比例，不是播放状态
//   audioplayer.onSeek(playId, tag, code, sec)
//   audioplayer.onEnd(playId, ...)           之后会跟一帧 onPlayProgress(playId, 0, 0)
//   audioplayer.stop(playId)                 call，手动切歌时发出
//   player.setInfo({playId: songId, songName, artistName, albumName, url})
//
// 曲目身份是 songId（playId 的前缀）。后缀是播放会话，seek 或重新缓冲时会变，不算切歌。
(function () {
  "use strict";

  var PORT = 52847;
  var TOKEN = "top-island-ncm-bridge-v1";

  if (!/^orpheus:\/\//.test(String(location.href))) return;
  if (window.__topIslandBridge) return;
  window.__topIslandBridge = true;

  var cur = {
    playId: "",
    songId: "",
    title: "",
    artist: "",
    album: "",
    coverUrl: "",
    durationMs: 0,
    positionMs: 0,
    anchorEpochMs: 0,
    playing: false,
  };
  var pendingMeta = {}; // setInfo 可能先于 load 到达，按 songId 暂存
  var prevPlayId = "";
  var prevSongId = "";
  var ended = false;
  var learned = { play: null, pause: null, seek: null };

  function songIdOf(playId) {
    var s = String(playId || "");
    var i = s.indexOf("_");
    var head = i >= 0 ? s.slice(0, i) : s;
    return /^\d+$/.test(head) ? head : "";
  }

  function hiResCover(url) {
    var u = String(url || "");
    return u ? u.split("?")[0] + "?param=512y512" : "";
  }

  function now() {
    return Date.now();
  }

  function owns(playId) {
    var pid = String(playId || "");
    if (!pid) return true;
    var sid = songIdOf(pid);
    if (sid && sid === cur.songId) {
      cur.playId = pid;
      return true;
    }
    // 切歌后上一曲的残留事件还会到几帧
    if (pid === prevPlayId || (sid && sid === prevSongId)) return false;
    enterTrack(pid);
    return true;
  }

  function enterTrack(playId) {
    var pid = String(playId || "");
    var sid = songIdOf(pid);
    if (sid !== cur.songId) {
      prevPlayId = cur.playId;
      prevSongId = cur.songId;
      cur.songId = sid;
      cur.title = "";
      cur.artist = "";
      cur.album = "";
      cur.coverUrl = "";
      cur.durationMs = 0;
      cur.positionMs = 0;
      var m = pendingMeta[sid];
      if (m) {
        applyMeta(m);
        delete pendingMeta[sid];
      }
    }
    cur.playId = pid;
    ended = false;
    cur.anchorEpochMs = now();
    cur.playing = true; // NCM 只在要播时才 load，若没播 onPlayState 会纠正
  }

  function applyMeta(info) {
    if (info.songName != null) cur.title = String(info.songName);
    if (info.artistName != null) cur.artist = String(info.artistName);
    if (info.albumName != null) cur.album = String(info.albumName);
    if (info.url) cur.coverUrl = hiResCover(info.url);
  }

  function onLoad(playId, info) {
    enterTrack(playId);
    if (info && info.duration != null) cur.durationMs = Math.round(Number(info.duration) * 1000);
    sendState();
  }

  function onPlayState(playId, _tag, state) {
    if (!owns(playId)) return;
    cur.playing = state === 1 || state === "1" || state === true;
    cur.anchorEpochMs = now();
    sendState();
  }

  function onPlayProgress(playId, positionSec) {
    if (!owns(playId)) return;
    if (ended) return;
    var n = Number(positionSec);
    if (!isFinite(n)) return;
    cur.positionMs = Math.max(0, Math.round(n * 1000));
    cur.anchorEpochMs = now();
    sendState();
  }

  function onSeek(playId, _tag, _code, targetSec) {
    if (!owns(playId)) return;
    cur.positionMs = Math.max(0, Math.round((Number(targetSec) || 0) * 1000));
    cur.anchorEpochMs = now();
    sendState();
  }

  function onEnd(playId) {
    if (songIdOf(playId) !== cur.songId) return;
    ended = true;
    cur.playing = false;
    cur.anchorEpochMs = now();
    sendState();
  }

  function onSetInfo(info) {
    if (!info || typeof info !== "object") return;
    var sid = String(info.playId || "");
    if (sid && sid !== cur.songId) {
      pendingMeta[sid] = info;
      return;
    }
    applyMeta(info);
    sendState();
  }

  var ws = null;
  var wsReady = false;
  var reconnectTimer = null;

  function connect() {
    try {
      ws = new WebSocket("ws://127.0.0.1:" + PORT + "/");
    } catch (e) {
      scheduleReconnect();
      return;
    }
    ws.onopen = function () {
      wsReady = true;
      safeSend({ type: "hello", token: TOKEN });
      seedFromPlaybackInfo();
      sendState();
    };
    ws.onmessage = function (ev) {
      try {
        handleControl(JSON.parse(ev.data));
      } catch (e) {}
    };
    ws.onclose = function () {
      wsReady = false;
      ws = null;
      scheduleReconnect();
    };
    ws.onerror = function () {
      try {
        ws.close();
      } catch (e) {}
    };
  }

  function scheduleReconnect() {
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(function () {
      reconnectTimer = null;
      connect();
    }, 3000);
  }

  function safeSend(obj) {
    if (!ws || !wsReady) return;
    try {
      ws.send(JSON.stringify(obj));
    } catch (e) {}
  }

  function sendState() {
    safeSend({
      type: "state",
      token: TOKEN,
      songId: cur.songId,
      playId: cur.playId,
      title: cur.title,
      artist: cur.artist,
      album: cur.album,
      coverUrl: cur.coverUrl,
      durationMs: cur.durationMs,
      positionMs: cur.positionMs,
      anchorEpochMs: cur.anchorEpochMs,
      playing: cur.playing,
    });
  }

  function rand() {
    return Math.random().toString(36).slice(2, 8);
  }

  // 参数形态照抄 NCM 自己的调用，只在还没从 observeCall 学到真实模板时用
  function buildGuess(verb, posSec) {
    var pid = cur.playId;
    var sid = cur.songId || songIdOf(pid);
    if (verb === "play") return [pid, pid + "|resume|" + rand()];
    if (verb === "pause") return [pid, sid + "|pause|" + rand()];
    if (verb === "seek") return [pid, sid + "|seek|" + rand(), posSec];
    return [pid];
  }

  function doControl(verb, posSec) {
    if (!window.channel || typeof window.channel.call !== "function") return;
    var args;
    var tmpl = learned[verb];
    if (Array.isArray(tmpl) && tmpl.length) {
      args = tmpl.slice();
      if (cur.playId) args[0] = cur.playId;
      if (verb === "seek" && args.length >= 3) args[2] = posSec;
    } else {
      args = buildGuess(verb, posSec);
    }
    try {
      window.channel.call("audioplayer." + verb, function () {}, args);
    } catch (e) {}
  }

  function handleControl(msg) {
    if (!msg || msg.type !== "control") return;
    if (msg.action === "play") doControl("play");
    else if (msg.action === "pause") doControl("pause");
    else if (msg.action === "seek") doControl("seek", Math.max(0, (msg.positionMs || 0) / 1000));
  }

  function observeCall(callArgs) {
    var name = callArgs[0];
    var payload = null;
    for (var i = 1; i < callArgs.length; i++) {
      if (typeof callArgs[i] !== "function") {
        payload = callArgs[i];
        break;
      }
    }
    if (name === "player.setInfo") {
      onSetInfo(payload);
    } else if (name === "audioplayer.load") {
      if (Array.isArray(payload) && payload.length) {
        enterTrack(payload[0]);
        sendState();
      }
    } else if (name === "audioplayer.stop") {
      if (Array.isArray(payload) && payload.length && songIdOf(payload[0]) === cur.songId) {
        onEnd(payload[0]);
      }
    } else if (name === "audioplayer.pause") {
      if (Array.isArray(payload)) learned.pause = payload;
    } else if (name === "audioplayer.play") {
      if (Array.isArray(payload)) learned.play = payload;
    } else if (name === "audioplayer.seek") {
      if (Array.isArray(payload)) learned.seek = payload;
    }
  }

  function seedFromPlaybackInfo() {
    try {
      if (!window.channel || typeof window.channel.call !== "function") return;
      window.channel.call(
        "audioplayer.getPlaybackInfo",
        function (info) {
          if (!info || !info.playId) return;
          if (!owns(info.playId)) return;
          if (info.playedTime != null) {
            cur.positionMs = Math.max(0, Math.round(Number(info.playedTime) * 1000));
            cur.anchorEpochMs = now();
          }
          sendState();
        },
        [{ playId: cur.playId }]
      );
    } catch (e) {}
  }

  var EVENT_HANDLERS = {
    "audioplayer.onLoad": function (a) {
      onLoad(a[0], a[1]);
    },
    "audioplayer.onPlayState": function (a) {
      onPlayState(a[0], a[1], a[2]);
    },
    "audioplayer.onPlayProgress": function (a) {
      onPlayProgress(a[0], a[1]);
    },
    "audioplayer.onSeek": function (a) {
      onSeek(a[0], a[1], a[2], a[3]);
    },
    "audioplayer.onEnd": function (a) {
      onEnd(a[0]);
    },
  };

  function hookChannel(channel) {
    if (channel.__topIslandHooked) return;

    var origRegister = channel.registerCall;
    if (typeof origRegister === "function") {
      channel.registerCall = function (name, cb) {
        try {
          var handler = EVENT_HANDLERS[name];
          if (handler && typeof cb === "function") {
            var inner = cb;
            var wrapped = function () {
              try {
                handler(Array.prototype.slice.call(arguments));
              } catch (e) {}
              return inner.apply(this, arguments);
            };
            return origRegister.call(this, name, wrapped);
          }
        } catch (e) {}
        return origRegister.apply(this, arguments);
      };
    }

    var origCall = channel.call;
    if (typeof origCall === "function") {
      channel.call = function () {
        try {
          observeCall(arguments);
        } catch (e) {}
        return origCall.apply(this, arguments);
      };
    }

    channel.__topIslandHooked = true;
  }

  var tries = 0;
  var timer = setInterval(function () {
    tries++;
    if (window.channel) {
      hookChannel(window.channel);
      if (!ws && !reconnectTimer) connect();
    }
    if (tries > 240) clearInterval(timer);
  }, 500);
})();
