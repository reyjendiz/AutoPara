// The class page's volume (the slider in the "Відкрита пара" bar), in every frame of the page.
// Meet plays a class through <audio> elements, Zoom's web client through Web Audio; both follow
// `level`. meeting.rs calls `__autoparaVolume(level)` on the top frame, which passes it down.
(() => {
  if (window.__autoparaVolume) return;
  let level = 1;
  // Each live context's speakers get one gain node in front of them.
  const gains = new Map();
  const { connect, disconnect } = AudioNode.prototype;
  const speakers = (node) => node instanceof AudioDestinationNode && node.context instanceof AudioContext;
  const gainFor = (destination) => {
    let gain = gains.get(destination);
    if (!gain) {
      gain = destination.context.createGain();
      gain.gain.value = level;
      connect.call(gain, destination);
      gains.set(destination, gain);
    }
    return gain;
  };
  AudioNode.prototype.connect = function (target, ...rest) {
    if (!speakers(target)) return connect.apply(this, [target, ...rest]);
    connect.apply(this, [gainFor(target), ...rest]);
    return target;
  };
  AudioNode.prototype.disconnect = function (...args) {
    if (speakers(args[0])) args[0] = gainFor(args[0]);
    return disconnect.apply(this, args);
  };

  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function (...args) {
    this.volume = level;
    return play.apply(this, args);
  };
  addEventListener("play", (event) => {
    if (event.target instanceof HTMLMediaElement) event.target.volume = level;
  }, true);

  const set = (value) => {
    level = value;
    for (const gain of gains.values()) gain.gain.value = value;
    for (const element of document.querySelectorAll("audio, video")) element.volume = value;
    for (let i = 0; i < frames.length; i++) frames[i].postMessage({ autoparaVolume: value }, "*");
  };
  window.__autoparaVolume = set;
  addEventListener("message", (event) => {
    const value = event.data?.autoparaVolume;
    if (event.source === parent && parent !== window && typeof value === "number" && value >= 0 && value <= 1) set(value);
    // A frame made after the last change asks for the level.
    if (event.data?.autoparaVolumeAsk && event.source) event.source.postMessage({ autoparaVolume: level }, "*");
  });
  if (parent !== window) parent.postMessage({ autoparaVolumeAsk: true }, "*");
})();
