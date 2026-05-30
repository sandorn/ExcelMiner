import { useState, useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/appStore';
import { formatElapsed } from '../utils/format';
import type { AggregationResult, Project } from '../types';

interface AggregationProgress {
    step: string;
    progress: number;
    status: string;
}

export function useAggregation() {
    const [results, setResults] = useState<AggregationResult[]>([]);
    const [isRunning, setIsRunning] = useState(false);
    const [progress, setProgress] = useState<string[]>([]);

    const unlistenRef = useRef<UnlistenFn | null>(null);

    // 组件卸载时取消监听
    useEffect(() => {
        return () => {
            unlistenRef.current?.();
        };
    }, []);

    const run = useCallback(async (project: Project, onLog: (msg: string) => void) => {
        setIsRunning(true);
        setProgress([]);
        const startTime = Date.now();
        onLog('=== 阶段1: 数据汇总 ===');

        let unlisten: UnlistenFn | null = null;
        try {
            unlisten = await listen<AggregationProgress>(
                'aggregation-progress',
                (event) => {
                    const msg = `  ${event.payload.step}`;
                    setProgress((prev) => [...prev, msg]);
                    onLog(msg);
                },
            );
            unlistenRef.current = unlisten;

            const engines = ['insurance', 'hotel', 'commercial', 'financial'];
            const newResults = await invoke<AggregationResult[]>('execute_aggregation', {
                project,
                engines,
            });

            // 同名引擎结果替换，保留不同名的已有结果
            const runNames = new Set(['保险数据汇总', '酒店数据汇总', '商写数据汇总', '经营报表汇总']);
            const storeResults = useAppStore.getState().aggregationResults;
            const merged = [
                ...storeResults.filter((r: AggregationResult) => !runNames.has(r.engine_name)),
                ...newResults,
            ];
            setResults(merged);
            useAppStore.getState().setAggregationResults(merged);

            const elapsed = formatElapsed(Date.now() - startTime);
            onLog(`=== 数据汇总完成，共耗时 ${elapsed} ===`);
            onLog('');
        } catch (e: any) {
            onLog(`错误: ${e}`);
            throw e; // 重新抛出，让调用方处理 UI 反馈
        } finally {
            unlisten?.();
            unlistenRef.current = null;
            setIsRunning(false);
        }
    }, []);

    return { results, isRunning, progress, run };
}
