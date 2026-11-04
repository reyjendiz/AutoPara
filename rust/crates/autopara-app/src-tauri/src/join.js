// Turns the camera and microphone off on the join screen of a class page in AutoPara's tab, and
// keeps them off until the user first clicks or types on the page (meeting.rs runs it on every
// page load). With "auto_join" (JOIN) it also joins, only once both devices are confirmed off or
// unavailable; anything unexpected leaves the page to the user. The hooks below were read from
// Meet's and Zoom's join screens in November 2026 and break when the sites change them.
(() => {
  if (window.__autoparaJoin) return;
  window.__autoparaJoin = true;
  const NAME = __NAME__;
  const JOIN = __JOIN__;
  const meet = location.hostname === "meet.google.com";
  const zoom = /(^|\.)zoom\.us$/.test(location.hostname) && location.pathname.includes("/wc/");
  if (!meet && !zoom) return;
  const $ = (selector) => document.querySelector(selector);
  const clicks = new Map();
  // The user's own input (our clicks are not trusted events): from then on the page is theirs.
  let touched = false;
  const touch = (event) => {
    if (event.isTrusted) touched = true;
  };
  addEventListener("pointerdown", touch, true);
  addEventListener("keydown", touch, true);

  // A toggle is clicked at most twice; a third time means it does not do what we expect.
  const press = (button) => {
    const count = (clicks.get(button) ?? 0) + 1;
    clicks.set(button, count);
    if (count > 2) throw new Error("toggle did not change");
    button.click();
  };

  // Meet: its "turn on microphone and camera" button means no device is in use at all.
  const meetOff = () => {
    if ($('[jsname="ZOOaZd"]')) return true;
    let off = true;
    for (const selector of ['[jsname="hw0c9"]', '[jsname="psRWwc"]']) {
      const toggle = $(selector);
      if (!toggle) return false;
      if (toggle.getAttribute("data-is-muted") !== "true") {
        press(toggle);
        off = false;
      }
    }
    return off;
  };

  // Zoom names the microphone icon after the action (Unmute = muted) and the camera after the state.
  const zoomOff = () => {
    const audio = $("#preview-audio-control-button");
    const video = $("#preview-video-control-button");
    const icon = (button) => button?.querySelector("svg")?.getAttribute("class") ?? "";
    if (!icon(audio) || !icon(video)) return false;
    let off = true;
    if (!icon(audio).startsWith("SvgAudioUnmute")) {
      press(audio);
      off = false;
    }
    if (!icon(video).startsWith("SvgVideoOff")) {
      press(video);
      off = false;
    }
    return off;
  };

  // Typed as real input: Meet ignores a value set from script.
  const named = (input) => {
    if (!input || input.value) return true;
    if (!NAME) return false;
    input.focus();
    if (!document.execCommand("insertText", false, NAME)) {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, NAME);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    return true;
  };

  let rounds = 0;
  const timer = setInterval(() => {
    try {
      // Without JOIN it keeps watching, as a site may switch a device on once permission is given.
      if (touched || ++rounds > (JOIN ? 120 : 1200)) throw new Error("left to the user");
      if (!(meet ? meetOff() : zoomOff()) || !JOIN) return;
      if (!named($(meet ? '[jsname="YPqjbf"]' : "#input-for-name"))) throw new Error("no name");
      const join = meet ? $('[data-promo-anchor-id="w5gBed"]') ?? $('[jsname="Qx7uuf"]') : $(".preview-join-button");
      if (!join || join.disabled || join.classList.contains("disabled")) return;
      clearInterval(timer);
      join.click();
    } catch {
      clearInterval(timer);
    }
  }, 500);
})();
