use crate::internal::*;

#[derive(Debug, Clone, new)]
pub struct Const {
    value: SharedTensor,
}

impl Const {
    pub fn for_tensor(tensor: Tensor) -> Const {
        Const { value: tensor.into() }
    }
}

impl Op for Const {
    fn name(&self) -> Cow<str> {
        "Const".into()
    }
}

impl StatelessOp for Const {
    fn eval(&self, _inputs: TVec<SharedTensor>) -> TractResult<TVec<SharedTensor>> {
        Ok(tvec![self.value.clone()])
    }
}

impl InferenceRulesOp for Const {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        _s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(&inputs, 0)?;
        check_output_arity(&outputs, 1)?;
        Ok(())
    }
}
