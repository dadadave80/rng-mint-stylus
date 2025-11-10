#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
extern crate alloc;

use alloc::vec::Vec;
use stylus_sdk::{
    alloy_primitives::{Address, U256},
    alloy_sol_types::sol,
    prelude::*,
    storage::{StorageAddress, StorageMap},
};

//*//////////////////////////////////////////////////////////////////////////
//                                 VRF SETUP
//////////////////////////////////////////////////////////////////////////*//

// Minimal interface for the Supra VRF Router Contract
// The `generateRequest` function is used to request randomness from Supra VRF
sol_interface! {
    interface ISupraRouterContract {
        function generateRequest(string memory function_sig, uint8 rng_count, uint256 num_confirmations, address client_wallet_address) external returns(uint256);
    }

    interface IErc20 {
        function mint(address account, uint256 value) external;
    }
}

sol! {
    // Thrown when a randomness request fails
    #[derive(Debug)]
    error RandomnessRequestFailed();
    // Thrown when a randomness request fails
    #[derive(Debug)]
    error MintFailed();
    // Thrown when a fulfillment is received from a non-Supra router
    #[derive(Debug)]
    error OnlySupraRouter();
}

// Custom events
sol! {
    event MintRequested(uint256 indexed nonce, address indexed to);
    event Minted(uint256 indexed nonce, address indexed to, uint256 amount);
}

#[derive(SolidityError, Debug)]
enum Error {
    // VRF Errors
    RandomnessRequestFailed(RandomnessRequestFailed),
    OnlySupraRouter(OnlySupraRouter),
    MintFailed(MintFailed),
}

//*//////////////////////////////////////////////////////////////////////////
//                               LOTTERY TOKEN
//////////////////////////////////////////////////////////////////////////*//

#[entrypoint]
#[storage]
struct LotteryToken {
    rng_token: StorageAddress,
    subscription_manager: StorageAddress,
    supra_router: StorageAddress,
    mint_address: StorageMap<U256, StorageAddress>,
}

#[public]
impl LotteryToken {
    #[constructor]
    pub fn constructor(
        &mut self,
        rng_token: Address,
        subscription_manager: Address,
        supra_router: Address,
    ) -> Result<(), Error> {
        self._init(rng_token, subscription_manager, supra_router)
    }

    pub fn mint_to(&mut self, to: Address) -> Result<(), Error> {
        self._mint_to(to)
    }

    // Callback function from Supra VRF, called when the randomness is fulfilled
    // This is not meant to be called by users
    pub fn mint_random_amount(&mut self, nonce: U256, rng_list: Vec<U256>) -> Result<(), Error> {
        self._mint_random_amount(nonce, rng_list)
    }
}

impl LotteryToken {
    fn _init(
        &mut self,
        rng_token: Address,
        subscription_manager: Address,
        supra_router: Address,
    ) -> Result<(), Error> {
        self.rng_token.set(rng_token);
        self.subscription_manager.set(subscription_manager);
        self.supra_router.set(supra_router);
        Ok(())
    }

    fn _mint_to(&mut self, to: Address) -> Result<(), Error> {
        let nonce = self._request_randomness()?;

        self.mint_address.setter(nonce).set(to);

        log(self.vm(), MintRequested { nonce, to });

        Ok(())
    }

    fn _mint_random_amount(&mut self, nonce: U256, rng_list: Vec<U256>) -> Result<(), Error> {
        // If the caller is not the Supra router, return an error
        if self.vm().msg_sender() != self.supra_router.get() {
            return Err(Error::OnlySupraRouter(OnlySupraRouter {}));
        }

        let receiver = self.mint_address.get(nonce);
        let random_num = rng_list[0];
        // Mint between 1 and 1,000 tokens
        let mint_range = U256::from(1000000000000000000000_u128);
        let mint_amount = (random_num % mint_range) + U256::from(1);

        let rng_token = IErc20::from(self.rng_token.get());
        let mint_request = rng_token.mint(&mut *self, receiver, mint_amount);

        if mint_request.is_err() {
            return Err(Error::MintFailed(MintFailed {}));
        }

        log(
            self.vm(),
            Minted {
                nonce,
                to: receiver,
                amount: mint_amount,
            },
        );

        Ok(())
    }

    fn _request_randomness(&mut self) -> Result<U256, Error> {
        let subscription_manager = self.subscription_manager.get();
        let supra_router_address = self.supra_router.get();
        let router = ISupraRouterContract::from(supra_router_address);
        let request_result = router.generate_request(
            &mut *self,
            String::from("mintRandomAmount(uint256,uint256[])"),
            1,
            U256::from(1),
            subscription_manager,
        );

        match request_result {
            Ok(nonce) => Ok(nonce),
            Err(_) => Err(Error::RandomnessRequestFailed(RandomnessRequestFailed {})),
        }
    }
}
