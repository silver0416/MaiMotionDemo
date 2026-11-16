---
title: 如何推算手順
description: 先由演算法算出初步的解法，再由 AI 模型調整成符合一般人自然打法的結果。
order: 2
keywords: [原理, 流程, 演算法, 成本, AI, TemotoAI, 怎麼算]
---

<IntroProgress />

MaiMotionDemo 的手順分兩層計算：先由演算法算出初步的解法，再由 AI 模型調整成符合一般人自然打法的結果。流程如下：

<Steps flow>
<Step icon="file-text">

**讀入譜面**：把 simai 譜面轉成每顆音符的時間與位置。

</Step>
<Step icon="scale" key layer="第一層・演算法">

**演算法推算**：替各種可能的打法算一個「成本」，例如手移動太急、雙手交叉、同一隻手太忙都會讓成本變高，取成本最低的作為初步解法。

</Step>
<Step icon="sparkles" key layer="第二層・AI 模型">

**AI 模型調整**：由 AI 模型（TemotoAI）把初步解法中不像真人的地方，調整成自然的打法。

</Step>
<Step icon="play">

**播放結果**：在盤面上顯示雙手的動作。

</Step>
</Steps>

<Mascot>

「成本」可以想成「這樣打有多費力」。

手要移動、交叉或一隻手忙不過來，這些情況都會讓路徑成本變高。

</Mascot>

其中，為了訓練第 3 步的 AI 模型對「真人怎麼打」的理解，我們需要玩家協助標註譜面。
