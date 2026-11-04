# Task page layout contract

Preserve existing requirements, support blocks, review controls and buttons.
Only summaries and links belong on the task page. Never paste a complete round here.
Replace TASK_ID, HISTORY_PAGE_ID and ARTICLE_PAGE_ID after checking ownership.
Do not save a concrete history page link in a shared database template.

## 轮次记录

<mention url="https://app.notion.com/p/HISTORY_PAGE_ID">本任务轮次记录</mention>

<toggle heading="h3">
第 ROUND 轮｜执行人：EXECUTOR｜日期：DATE｜结果：RESULT

SUMMARY

完整文章：<mention url="https://app.notion.com/p/ARTICLE_PAGE_ID" />
</toggle>

Keep one ordered summary per round. Each article is a child of this task's fixed
history index and contains the complete report, evidence and consumed instructions.
Reuse the same article for continuation of the same round.
