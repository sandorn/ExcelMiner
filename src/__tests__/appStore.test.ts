import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../stores/appStore';

describe('appStore', () => {
  beforeEach(() => {
    // 重置 store 到初始状态
    useAppStore.setState({
      project: null,
      projectName: '',
      appConfig: null,
      currentStep: 0,
      aggregationResults: [],
      analysisResults: [],
      lastError: null,
    });
  });

  it('should initialize with null project', () => {
    expect(useAppStore.getState().project).toBeNull();
  });

  it('should initialize with empty project name', () => {
    expect(useAppStore.getState().projectName).toBe('');
  });

  it('should set project and auto-set projectName', () => {
    useAppStore.getState().setProject({ name: 'test' } as any);
    expect(useAppStore.getState().project?.name).toBe('test');
    expect(useAppStore.getState().projectName).toBe('test');
  });

  it('should clear project', () => {
    useAppStore.getState().setProject({ name: 'test' } as any);
    useAppStore.getState().setProject(null);
    expect(useAppStore.getState().project).toBeNull();
    expect(useAppStore.getState().projectName).toBe('');
  });

  it('should set and get aggregationResults', () => {
    const results = [{ engine_name: '保险数据汇总', companies_processed: 3 }];
    useAppStore.getState().setAggregationResults(results);
    expect(useAppStore.getState().aggregationResults).toEqual(results);
  });

  it('should set and get analysisResults', () => {
    const results = [{ company_name: 'test', content: '分析内容' }];
    useAppStore.getState().setAnalysisResults(results);
    expect(useAppStore.getState().analysisResults).toEqual(results);
  });

  it('should set and clear lastError', () => {
    const error = { code: 'API_KEY', message: '无效' };
    useAppStore.getState().setLastError(error);
    expect(useAppStore.getState().lastError).toEqual(error);
    useAppStore.getState().setLastError(null);
    expect(useAppStore.getState().lastError).toBeNull();
  });

  it('should set currentStep', () => {
    useAppStore.getState().setCurrentStep(2);
    expect(useAppStore.getState().currentStep).toBe(2);
  });
});
