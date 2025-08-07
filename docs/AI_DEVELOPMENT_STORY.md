# AI-Assisted Development Story: Building all-smi

**Author:** Jeongkyu Shin  
**Date:** July 7, 2025

Last weekend, I released a new open-source GPU monitoring tool called all-smi - a unified monitoring solution for GPUs and NPUs across multiple servers. Currently supports NVIDIA GPUs, NVIDIA Jetson GPUs with DLA modules, and Apple Silicon GPU/ANE monitoring. (more to come soon.)

The pain of installing multiple monitoring tools and jumping between different viewers just to check GPU status drove me to build this. The core feature is running API daemons across server nodes and aggregating everything in one view - designed to handle up to 300 nodes and 2,400 GPUs.

Installation:
* cargo install all-smi
* Or on macOS: brew tap lablup/tap && brew install all-smi
* Binaries & Packages: https://github/inureyes/all-smi

Here's the real story though - I actually started this last August during vacation, thinking "maybe I should learn Rust." Back then, we had a mess of different GPUs and NPUs with no decent unified viewing tools, each with completely different interfaces. Rust seemed like a good choice simply because I'd never really used it beyond reading the Book of Rust in its pre-1.0 days.

I spent three days coding with Google AI Studio and ChatGPT, feeding them prompts like: 
"I'm almost completely new to Rust but comfortable with C and Python 3. I have extensive experience with async programming, packaging, and UI technologies like jQuery and web components. When I hit errors, explain them considering my background, use analogies I'll understand, and if it's a human error due to Rust's unique features, explain that in detail."

Then vacation ended, work resumed, and the code sat untouched for nearly a year. Last weekend, facing the prospect of monitoring four-digit GPU counts and really not wanting to set up DCGM and Grafana dashboards yet again, I picked it back up.

This time I had Gemini CLI as my companion (switching to Claude Code when needed). While working on presentation slides, I coded until it reached a releasable state.

Today's development reinforced something: the deeper your understanding of code, computing, and networking, the better you can guide AI coding assistants. When I guided Gemini and Claude Code on implementing double buffering in the terminal, managing multiple screen buffers for composition, and solving frame flickering issues, I realized something. These AI models can dive deep when given technical direction, but without it, they produce mediocre solutions. They're somewhat weak-willed - they'll output code following instructions but won't deeply consider implementation details on their own. The boundary of AI coding capability seems tightly bound by the expertise of the person guiding it.

Strong direction creates the will to dig into problems. For AI and humans alike.

P.S. Gemini CLI, when you're not giving bizarre responses and maintaining your sanity, you're the best...

#OpenSource #Rust #GPU #Monitoring #AIAssistedDevelopment #DevOps

---

*Original post: https://www.linkedin.com/posts/jeongkyu_opensource-rust-gpu-activity-7349675474077294592-PhEr*
