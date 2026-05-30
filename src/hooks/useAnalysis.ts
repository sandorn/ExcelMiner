import { useState, useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/appStore';
import { formatElapsed, formatProgress } from '../utils/format';
import type { AnalysisResult, ProgressUpdate, Project } from '../types';

export function useAnalysis() {
    const [results, setResults] = useState<AnalysisResult[]>([]);
    const [isRunning, setIsRunning] = useState(false);
    const [runningLabel, setRunningLabel] = useState('');
    const [progress, setProgress] = useState<string[]>([]);

    const unlistenRef = useRef<UnlistenFn | null>(null);

    // 组件卸载时取消监听
    useEffect(() => {
        return () => {
            unlistenRef.current?.();
        };
    }, []);

    const startListening = useCallback(
        async (onLog: (msg: string) => void, onProgress?: (payload: ProgressUpdate) => string) => {
            const unlisten = await listen<ProgressUpdate>('analysis-progress', (event) => {
                // 跳过 done 状态事件（"板块分析完成"/"经营指标分析完成"由阶段总结行替代）
                if (event.payload.status === 'done') return;
                const msg: string = onProgress ? onProgress(event.payload) : `  ${event.payload.step}`;
                setProgress((prev) => [...prev, msg]);
                onLog(msg);
            });
            unlistenRef.current = unlisten;
            return unlisten;
        },
        [],
    );

    /** 阶段2: 板块业态分析 */
    const runSegment = useCallback(async (project: Project, onLog: (msg: string) => void) => {
        setIsRunning(true);
        setRunningLabel('业态分析');
        setProgress([]);
        const startTime = Date.now();
        onLog('=== 阶段2: 板块AI分析 ===');

        let unlisten: UnlistenFn | null = null;
        try {
            const totalSegments = 3; // Insurance / Hotel / Commercial
            unlisten = await startListening(
                onLog,
                (payload: ProgressUpdate) => {
                    const current = Math.round(payload.progress * totalSegments);
                    const done = payload.step.includes('分析完成');
                    return `  ${formatProgress(current, totalSegments, startTime, done)} ${payload.step}`;
                },
            );

            const newResults = await invoke<AnalysisResult[]>('execute_segment_analysis', {
                project,
                businessTypes: ['Commercial', 'Insurance', 'Hotel'],
                customPrompt: null,
            });
            const current = useAppStore.getState().analysisResults;
            const merged = [
                ...current.filter((r: AnalysisResult) => r.analysis_category === 'company'),
                ...newResults,
            ];
            setResults(merged);
            useAppStore.getState().setAnalysisResults(merged);

            const elapsed = formatElapsed(Date.now() - startTime);
            onLog(`=== 业态分析完成，共耗时 ${elapsed} ===`);
            onLog('');
        } catch (e: any) {
            onLog(`错误: ${e}`);
            throw e;
        } finally {
            unlisten?.();
            unlistenRef.current = null;
            setIsRunning(false);
            setRunningLabel('');
        }
    }, [startListening]);

    /** 阶段3: 子公司经营指标分析 */
    const runCompany = useCallback(async (project: Project, onLog: (msg: string) => void) => {
        setIsRunning(true);
        setRunningLabel('公司分析');
        setProgress([]);
        const startTime = Date.now();
        onLog('=== 阶段3: 公司核心指标分析 ===');

        let unlisten: UnlistenFn | null = null;
        try {
            const totalCompanies = project.companies.length;
            unlisten = await startListening(
                onLog,
                (payload: ProgressUpdate) => {
                    const current = Math.round(payload.progress * totalCompanies);
                    const done = payload.step.includes('分析完成');
                    return `  ${formatProgress(current, totalCompanies, startTime, done)} ${payload.step}`;
                },
            );

            const newResults = await invoke<AnalysisResult[]>('execute_company_analysis', {
                project,
            });
            const current = useAppStore.getState().analysisResults;
            const merged = [
                ...current.filter((r: AnalysisResult) => r.analysis_category === 'segment'),
                ...newResults,
            ];
            setResults(merged);
            useAppStore.getState().setAnalysisResults(merged);

            const elapsed = formatElapsed(Date.now() - startTime);
            onLog(`=== 公司分析完成，共耗时 ${elapsed} ===`);
            onLog('');
        } catch (e: any) {
            onLog(`错误: ${e}`);
            throw e;
        } finally {
            unlisten?.();
            unlistenRef.current = null;
            setIsRunning(false);
            setRunningLabel('');
        }
    }, [startListening]);

    return { results, isRunning, runningLabel, progress, runSegment, runCompany };
}
