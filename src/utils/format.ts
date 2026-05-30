/** 格式化耗时 (与 AardMiner 一致: X分XX秒) */
export function formatElapsed(ms: number): string {
    const sec = Math.floor(ms / 1000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}分${String(s).padStart(2, '0')}秒`;
}

/** 时间戳 HH:MM:SS */
export function timestamp(): string {
    const d = new Date();
    return [d.getHours(), d.getMinutes(), d.getSeconds()]
        .map((n) => String(n).padStart(2, '0'))
        .join(':');
}

/** 格式化进度前缀 [N/T]（仅在完成行附加已用时） */
export function formatProgress(current: number, total: number, startTime: number, showElapsed: boolean): string {
    if (showElapsed) {
        const elapsed = Date.now() - startTime;
        return `[${current}/${total}] 已用时: ${formatElapsed(elapsed)}`;
    }
    return `[${current}/${total}]`;
}
