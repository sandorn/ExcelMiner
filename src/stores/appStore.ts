import { create } from 'zustand';
import type { Project, AppConfig, AppError } from '../types';

interface AppState {
    // 项目
    project: Project | null;
    projectName: string;
    setProject: (p: Project | null) => void;

    // 配置
    appConfig: AppConfig | null;
    setAppConfig: (c: AppConfig) => void;

    // 当前步骤
    currentStep: number;
    setCurrentStep: (n: number) => void;

    // 汇总结果
    aggregationResults: any[];
    setAggregationResults: (r: any[]) => void;

    // 分析结果
    analysisResults: any[];
    setAnalysisResults: (r: any[]) => void;

    // 最近错误
    lastError: AppError | null;
    setLastError: (e: AppError | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
    project: null,
    projectName: '',
    setProject: (p) => set({ project: p, projectName: p?.name ?? '' }),

    appConfig: null,
    setAppConfig: (c) => set({ appConfig: c }),

    currentStep: 0,
    setCurrentStep: (n) => set({ currentStep: n }),

    aggregationResults: [],
    setAggregationResults: (r) => set({ aggregationResults: r }),

    analysisResults: [],
    setAnalysisResults: (r) => set({ analysisResults: r }),

    lastError: null,
    setLastError: (e) => set({ lastError: e }),
}));
