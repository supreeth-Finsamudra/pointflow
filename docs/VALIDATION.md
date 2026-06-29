# PointFlow — Demand Validation Kit

Goal: cheaply test whether the **"dictate/type from your phone into any computer —
even a locked-down RDP/Citrix desktop where clipboard tools fail"** wedge has
willing-to-pay users, **before** writing more product code. Timebox: ~2 weeks.

Decision this informs: build the subscription dictation product (real swing) vs.
fall back to shipping the $4 consumer app vs. open-source it.

---

## 1. The positioning to test

**One-liner:** *Use your phone to talk or type into any computer — even your
locked-down work or remote desktop, where copy-paste and dictation are blocked.*

**Why this and not "phone as mouse":** the plain remote-trackpad is a $4 commodity
(Tracepad, Mouse·Keyboard at 4.7★/$4.99). The defensible, payable wedge is the
intersection nobody owns:
- **Wispr Flow** ($2B trajectory) — dictation king, but pastes via clipboard → **breaks on RDP/Citrix** (admitted in their own docs).
- **DictaFlow** ($7/mo, solo) — RDP-proof dictation, but clunky capture (Telegram), no trackpad, thin polish.
- **Remote Mouse** — trackpad+keyboard, clipboard-based, insecure, no real dictation.
- **PointFlow** — already uses **keystroke injection (not clipboard)** + trackpad + phone keyboard (voice/swipe/emoji). Architecturally ahead of Wispr for locked-down, ahead of DictaFlow on input/UX.

**The edge we must prove people want** (bare wedge is taken by DictaFlow, so we
need one of these to be the headline): (a) **on-device / compliant dictation** —
audio never leaves the secure container (named, unsolved gap for HIPAA/DLP); or
(b) **materially better reliability + capture UX** (your phone is the device, no
Telegram detour).

> Name note: "PointFlow" reads pointer-first. If dictation becomes the hero, a
> name leaning into "type/talk anywhere" may convert better. Don't rename yet —
> test the message first.

---

## 2. Landing-page smoke test

Stand up a one-page site (Carrd, or a Vercel static page) — **half a day, no product wiring.**

**Structure:**
- **Hero:** the one-liner above + a 15-sec screen-capture GIF of dictating on the phone → text landing in a Mac app (you already have the working product — record it).
- **Problem section:** "Stuck on a Citrix/RDP/work desktop where dictation breaks and copy-paste is blocked? Your phone is the way in."
- **3 value props:** works where clipboard is blocked · your phone's voice + swipe + emoji + any language · nothing to install on the locked machine.
- **(Edge) trust line:** "Audio stays on your device" / "end-to-end encrypted, no server" — only if you'll actually build it.
- **CTA — the actual test:** "Get early access" → email capture (Tally/Formspree). Add a **fake pricing block** ($9/mo) with a "Start free trial" button that goes to the email form. Clicks on the *paid* button = the real willingness-to-pay signal, not just generic signups.

**What counts as signal (per ~100 targeted visitors):**
- **Weak:** <3% email signups → message isn't landing.
- **Promising:** >8% signups AND several clicking the *paid* CTA.
- **Strong:** people email you unprompted asking "when can I buy this / does it work with [Epic/Citrix]."

---

## 3. Where the users actually are (not HN)

Go where the locked-down pain lives. **Read each community's self-promo rules first; lead with the problem, not your link.**

- **r/Citrix, r/sysadmin, r/healthIT** — search existing threads on "dictation Citrix", "clipboard disabled RDP". Comment helpfully; DM people who described the pain.
- **Healthcare:** r/medicine, r/Epic (EHR), physician Facebook/Slack groups — clinicians dictating into Citrix-hosted Epic are the highest-pain, highest-pay segment (but sell to *individuals*, not hospital IT).
- **Legal:** r/Lawyertalk, legaltech forums — digital dictation is a budgeted habit there.
- **Accessibility / RSI:** r/RSI, r/accessibility, Talon Slack — high need, lower volume; good for empathy + early evangelists.
- **Remote/VDI workers & BPO:** r/remotework, offshore/VDI groups — strongest evidence of the *locked-down condition*, unproven they buy. Probe, don't assume.

Also: search Twitter/X and Reddit for live complaints ("dictation doesn't work in remote desktop") and reply with a "building something for this — can I ask about your setup?"

---

## 4. Problem interviews (the real validation) — aim for 8–10

Landing pages measure clicks; interviews tell you *why*. Use **Mom Test** rules:
ask about their **past behavior and real workflow**, never pitch the idea or ask
"would you use this?" (everyone lies politely).

**Script:**
1. "Walk me through the last time you had to enter a lot of text into your remote/Citrix/work desktop. What was that like?"
2. "When dictation or copy-paste doesn't work there, what do you actually do?" *(listen for real hacks — phone notes + retype, asking IT, giving up)*
3. "What have you tried to fix it? Did you pay for anything? What happened?"
4. "How often does this hit you, and roughly how much time does it cost?"
5. "If your audio had to leave the secure machine, would that be a problem?" *(probes the compliance edge)*
6. Only at the very end, show the 15-sec demo and watch their face. Then: "What would have to be true for you to use this at work?"

**Greenlight if:** ≥5 of 10 describe the pain unprompted, *already hack around it
today*, and either pay for a workaround or say their employer does. **Kill / pivot
if:** people shrug, have no current workaround, or say "I'd just use the keyboard."

---

## 5. Two-week plan

- **Days 1–2:** record the demo GIF; write + ship the landing page with email + fake-pricing CTA.
- **Days 3–10:** seed 5–8 communities (helpful comments, not spam); start DMs; book interviews.
- **Days 8–14:** run interviews; tally landing-page conversions.
- **End:** decide — real swing (build the compliant/reliable dictation product) · ship the $4 consumer app · or open-source and move on.

**Cheapest possible version:** skip the landing page entirely and just do 10
interviews. If the pain isn't vivid in conversations, no landing page will save it.
