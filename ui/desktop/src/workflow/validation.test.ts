import { describe, it, expect } from 'vitest';
import { getWorkflowJsonSchema } from './validation';

describe('Workflow Validation', () => {
  describe('getWorkflowJsonSchema', () => {
    it('returns a valid JSON schema object', () => {
      const schema = getWorkflowJsonSchema();

      expect(schema).toBeDefined();
      expect(typeof schema).toBe('object');
      expect(schema).toHaveProperty('$schema');
      expect(schema).toHaveProperty('type');
      expect(schema).toHaveProperty('title');
      expect(schema).toHaveProperty('description');
    });

    it('includes standard JSON Schema properties', () => {
      const schema = getWorkflowJsonSchema();

      expect(schema.$schema).toBe('http://json-schema.org/draft-07/schema#');
      expect(schema.title).toBeDefined();
      expect(schema.description).toBeDefined();
    });

    it('returns consistent schema across calls', () => {
      const schema1 = getWorkflowJsonSchema();
      const schema2 = getWorkflowJsonSchema();

      expect(schema1).toEqual(schema2);
    });
  });
});
